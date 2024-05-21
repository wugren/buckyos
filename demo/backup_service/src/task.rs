use std::{
    path::PathBuf,
    sync::{Arc, Mutex, Weak},
};

use base58::ToBase58;
use futures::SinkExt;
use sha2::Digest;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use backup_lib::{CheckPointVersion, ChunkInfo, FileInfo, TaskId, TaskInfo, TaskKey};

use crate::task_mgr::BackupTaskMgrInner;

#[derive(Clone)]
pub(crate) enum BackupTaskEvent {
    New(BackupTask),
    Idle(BackupTask),
    ErrorAndWillRetry(BackupTask),
    Fail(BackupTask),
    Successed(BackupTask),
}

pub(crate) enum BackupTaskControl {
    Stop,
}

pub trait Task {
    fn task_key(&self) -> TaskKey;
    fn task_id(&self) -> TaskId;
    fn check_point_version(&self) -> CheckPointVersion;
    fn prev_check_point_version(&self) -> Option<CheckPointVersion>;
    fn meta(&self) -> Option<String>;
    fn dir_path(&self) -> PathBuf;
    fn is_all_files_ready(&self) -> bool;
    fn is_all_files_done(&self) -> bool;
    fn file_count(&self) -> usize;
    fn start(&self) -> Result<(), Box<dyn std::error::Error>>;
    fn stop(&self) -> Result<(), Box<dyn std::error::Error>>;
}

#[derive(Clone)]
pub struct BackupTask {
    mgr: Weak<BackupTaskMgrInner>,
    info: Arc<Mutex<TaskInfo>>,
    control: (
        tokio::sync::mpsc::Sender<BackupTaskControl>,
        tokio::sync::mpsc::Receiver<BackupTaskControl>,
    ),
    uploading_chunks: Arc<Mutex<Vec<ChunkInfo>>>,
}

impl BackupTask {
    pub(crate) fn from_storage(mgr: Weak<BackupTaskMgrInner>, info: TaskInfo) -> Self {
        Self {
            mgr,
            info: Arc::new(Mutex::new(info)),
            uploading_chunks: Arc::new(Mutex::new(Vec::new())),
            control: tokio::sync::mpsc::channel(1024),
        }
    }

    pub(crate) async fn create_new(
        mgr: Weak<BackupTaskMgrInner>,
        task_key: TaskKey,
        check_point_version: CheckPointVersion,
        prev_check_point_version: Option<CheckPointVersion>,
        meta: Option<String>,
        dir_path: PathBuf,
        files: Vec<(PathBuf, Option<(String, u64)>)>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let chunk_file_info_rets = futures::future::join_all(files.into_iter().enumerate().map(
            |(seq, chunk_relative_path, hash_and_size)| {
                let dir_path = dir_path.clone();
                async move {
                    match hash_and_size {
                        Some((hash, file_size)) => Ok(FileInfo {
                            task_id: TaskId::from(0),
                            file_seq: seq,
                            file_path: chunk_relative_path,
                            hash,
                            file_size,
                            file_seq: todo!(),
                        }),
                        None => {
                            // TODO: read by chunks
                            let chunk_full_path = dir_path.join(&chunk_relative_path);
                            let mut file = tokio::fs::File::open(chunk_full_path).await?;
                            let file_size = file.metadata().await?.len();
                            let mut buf = vec![];
                            file.read_to_end(&mut buf).await?;

                            let mut hasher = sha2::Sha256::new();
                            hasher.update(buf.as_slice());
                            let hash = hasher.finalize();
                            let hash = hash.as_slice().to_base58();

                            Ok(FileInfo {
                                task_id: TaskId::from(0),
                                file_seq: seq,
                                file_path: chunk_relative_path,
                                hash,
                                file_size,
                            })
                        }
                    }
                }
            },
        ))
        .await;

        let mut files = vec![];
        for info in chunk_file_info_rets {
            match info {
                Err(err) => {
                    log::error!("read chunk files failed: {:?}", err);
                    return Err(err);
                }
                Ok(r) => files.push(r),
            }
        }

        let task_storage = mgr
            .upgrade()
            .map_or(
                Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "maybe the system has stopped.",
                ))),
                |t| Ok(t),
            )?
            .task_storage();

        let task_id = task_storage
            .create_task_with_files(
                &task_key,
                check_point_version,
                prev_check_point_version,
                meta.as_ref().map(|p| p.as_str()),
                dir_path.as_path(),
                files.as_slice(),
            )
            .await?;

        Ok(Self {
            mgr,
            info: Arc::new(Mutex::new(TaskInfo {
                task_id,
                task_key,
                check_point_version,
                prev_check_point_version,
                meta,
                dir_path: dir_path,
                is_all_files_ready: false,
                is_all_files_done: false,
                file_count: files.len(),
            })),
            uploading_chunks: Arc::new(Mutex::new(vec![])),
            control: tokio::sync::mpsc::channel(1024),
        })
    }

    // TODO: error handling
    async fn run_once(&self) -> Result<BackupTaskEvent, Box<dyn std::error::Error>> {
        let task_mgr = match self.mgr.upgrade() {
            Some(mgr) => mgr,
            None => {
                log::error!("task manager has been dropped.");
                return Ok(BackupTaskEvent::ErrorAndWillRetry(self.clone()));
            }
        };

        let task_storage = task_mgr.task_storage();
        let task_info = self.info.lock().unwrap().clone();

        // push task info
        let (remote_task_mgr, remote_task_id) = match task_storage
            .is_task_info_pushed(&task_key, check_point_version)
            .await
        {
            Ok(remote_task_id) => {
                let remote_task_mgr = match task_mgr
                    .task_mgr_selector()
                    .select(&task_info.task_key, task_info.check_point_version)
                    .await
                {
                    Ok(remote_task_mgr) => remote_task_mgr,
                    Err(_) => return Ok(BackupTaskEvent::ErrorAndWillRetry(self.clone())),
                };

                let remote_task_id = match remote_task_id {
                    Some(remote_task_id) => remote_task_id,
                    None => {
                        match remote_task_mgr
                            .push_task_info(
                                &task_info.task_key,
                                task_info.check_point_version,
                                task_info.prev_check_point_version,
                                task_info.meta.as_ref().map(|s| s.as_str()),
                                task_info.dir_path.as_path(),
                            )
                            .await
                        {
                            Ok(remote_task_id) => {
                                if let Err(err) = task_mgr
                                    .task_storage()
                                    .task_info_pushed(
                                        &task_key,
                                        check_point_version,
                                        remote_task_id,
                                    )
                                    .await
                                {
                                    return Ok(BackupTaskEvent::ErrorAndWillRetry(self.clone()));
                                }
                                remote_task_id
                            }
                            Err(_) => return Ok(BackupTaskEvent::ErrorAndWillRetry(self.clone())),
                        }
                    }
                };

                (remote_task_mgr, remote_task_id)
            }
            Err(err) => return Ok(BackupTaskEvent::ErrorAndWillRetry(self.clone())),
        };

        // push files
        // TODO: multiple files
        loop {
            let upload_files = match task_storage.get_incomplete_files(0, 1).await {
                Ok(files) => {
                    if files.len() == 0 {
                        if self.is_all_files_ready() {
                            return Ok(BackupTaskEvent::Successed(self.clone()));
                        } else {
                            return Ok(BackupTaskEvent::Idle(self.clone()));
                        }
                    }
                    files
                }
                Err(err) => return Ok(BackupTaskEvent::ErrorAndWillRetry(self.clone())),
            };

            for file in upload_files {
                let (file_server_type, file_server_name, chunk_size) = match task_storage
                    .is_file_info_pushed(
                        &task_info.task_key,
                        task_info.check_point_version,
                        file.file_path.as_path(),
                    )
                    .await
                {
                    Ok(file_server_name) => match file_server_name {
                        Some(file_server_name) => file_server_name,
                        None => {
                            match remote_task_mgr
                                .add_file(
                                    remote_task_id,
                                    file.file_path.as_path(),
                                    file.hash.as_str(),
                                    file.file_size,
                                )
                                .await
                            {
                                Ok((file_server_type, file_server_name, chunk_size)) => {
                                    match task_storage
                                        .file_info_pushed(
                                            &task_info.task_key,
                                            task_info.check_point_version,
                                            file.file_path.as_path(),
                                            file_server_type,
                                            file_server_name.as_str(),
                                            chunk_size,
                                        )
                                        .await
                                    {
                                        Ok(_) => (file_server_type, file_server_name),
                                        Err(_) => {
                                            return Ok(BackupTaskEvent::ErrorAndWillRetry(
                                                self.clone(),
                                            ))
                                        }
                                    }
                                }
                                Err(e) => {
                                    return Ok(BackupTaskEvent::ErrorAndWillRetry(self.clone()))
                                }
                            }
                        }
                    },
                    Err(_) => return Ok(BackupTaskEvent::ErrorAndWillRetry(self.clone())),
                };

                let remote_file_server = match task_mgr
                    .file_mgr_selector()
                    .select_by_name(file_server_type, file_server_name.as_str())
                    .await
                {
                    Ok(remote_file_server) => remote_file_server,
                    Err(err) => return Ok(BackupTaskEvent::ErrorAndWillRetry(self.clone())),
                };

                // push chunks
                let file_storage = task_mgr.file_storage();
                let chunk_size = chunk_size as u64;
                let chunk_count = (file.file_size + chunk_size - 1) / chunk_size;
                let file_path = task_info.dir_path.join(file.file_path.as_path());
                for chunk_seq in 0..chunk_count {
                    let offset = chunk_seq * chunk_size;
                    let chunk_size = std::cmp::min(chunk_size, file.file_size - offset);
                    let (chunk_server_type, chunk_server_name, chunk_hash, chunk) =
                        match file_storage
                            .is_chunk_info_pushed(file.hash.as_str(), chunk_seq)
                            .await
                        {
                            Ok(chunk_server) => match chunk_server {
                                Some((chunk_server_type, chunk_server_name, chunk_hash)) => {
                                    (chunk_server_type, chunk_server_name, chunk_hash, None)
                                }
                                None => {
                                    match read_file_from(file_path.as_path(), offset, chunk_size)
                                        .await
                                    {
                                        Ok(chunk) => {
                                            let mut hasher = sha2::Sha256::new();
                                            hasher.update(chunk.as_slice());
                                            let hash = hasher.finalize();
                                            let hash = hash.as_slice().to_base58();
                                            match remote_file_server
                                                .add_chunk(
                                                    file.hash.as_str(),
                                                    chunk_seq,
                                                    hash.as_str(),
                                                )
                                                .await
                                            {
                                                Ok((chunk_server_type, chunk_server_name)) => {
                                                    match file_storage
                                                        .chunk_info_pushed(
                                                            file.hash.as_str(),
                                                            chunk_seq,
                                                            chunk_server_type,
                                                            chunk_server_name.as_str(),
                                                            hash.as_str(),
                                                        )
                                                        .await
                                                    {
                                                        Ok(_) => (
                                                            chunk_server_type,
                                                            chunk_server_name,
                                                            hash,
                                                            Some(chunk),
                                                        ),
                                                        Err(err) => {
                                                            return Ok(
                                                                BackupTaskEvent::ErrorAndWillRetry(
                                                                    self.clone(),
                                                                ),
                                                            )
                                                        }
                                                    }
                                                }
                                                Err(err) => {
                                                    return Ok(BackupTaskEvent::ErrorAndWillRetry(
                                                        self.clone(),
                                                    ))
                                                }
                                            }
                                        }
                                        Err(err) => {
                                            return Ok(BackupTaskEvent::ErrorAndWillRetry(
                                                self.clone(),
                                            ))
                                        }
                                    }
                                }
                            },
                            Err(err) => {
                                return Ok(BackupTaskEvent::ErrorAndWillRetry(self.clone()))
                            }
                        };

                    let chunk_storage = task_mgr.chunk_storage();
                    match chunk_storage.is_chunk_uploaded(chunk_hash.as_str()).await {
                        Ok(is_upload) => {
                            if is_upload {
                                continue;
                            }
                        }

                        Err(err) => return Ok(BackupTaskEvent::ErrorAndWillRetry(self.clone())),
                    }

                    let remote_chunk_server = match task_mgr
                        .chunk_mgr_selector()
                        .select_by_name(chunk_server_type, chunk_server_name.as_str())
                        .await
                    {
                        Ok(remote_chunk_server) => remote_chunk_server,
                        Err(err) => return Ok(BackupTaskEvent::ErrorAndWillRetry(self.clone())),
                    };

                    let chunk = match chunk {
                        Some(chunk) => chunk,
                        None => match read_file_from(file_path.as_path(), offset, chunk_size).await
                        {
                            Ok(chunk) => chunk,
                            Err(err) => {
                                return Ok(BackupTaskEvent::ErrorAndWillRetry(self.clone()))
                            }
                        },
                    };

                    match remote_chunk_server.upload(chunk_hash.as_str(), chunk.as_slice()) {
                        Ok(_) => {
                            if let Err(err) =
                                chunk_storage.chunk_uploaded(chunk_hash.as_str()).await
                            {
                                return Ok(BackupTaskEvent::ErrorAndWillRetry(self.clone()));
                            }
                        }
                        Err(err) => return Ok(BackupTaskEvent::ErrorAndWillRetry(self.clone())),
                    }
                }
            }
        }
    }
}

async fn read_file_from(
    file_path: &Path,
    offset: u64,
    len: u64,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut file = tokio::fs::File::open(file_path).await?;
    file.seek(std::io::SeekFrom::Start(offset)).await?;

    let mut buf = Vec::with_capacity(len);
    unsafe {
        buf.set_len(len);
    }
    file.read_exact(buf.as_mut_slice()).await?;

    Ok(buf)
}

impl Task for BackupTask {
    fn task_key(&self) -> TaskKey {
        self.info.lock().unwrap().task_key.clone()
    }

    fn task_id(&self) -> TaskId {
        self.info.lock().unwrap().task_id.clone()
    }

    fn check_point_version(&self) -> CheckPointVersion {
        self.info.lock().unwrap().check_point_version
    }

    fn prev_check_point_version(&self) -> Option<CheckPointVersion> {
        self.info.lock().unwrap().prev_check_point_version
    }

    fn meta(&self) -> Option<String> {
        self.info.lock().unwrap().meta.clone()
    }

    fn dir_path(&self) -> PathBuf {
        self.info.lock().unwrap().dir_path.clone()
    }

    fn is_all_files_ready(&self) -> bool {
        self.info.lock().unwrap().is_all_files_ready
    }

    fn is_all_files_done(&self) -> bool {
        self.info.lock().unwrap().is_all_files_done
    }

    fn file_count(&self) -> usize {
        self.info.lock().unwrap().file_count
    }

    fn start(&self) {
        let backup_task = self.clone();
        tokio::task::spawn(async move {
            loop {
                let task_mgr = backup_task.mgr.upgrade();
                let task_mgr = match task_mgr {
                    Some(task_mgr) => task_mgr,
                    None => {
                        log::error!("task manager has been dropped.");
                        break;
                    }
                };

                // run once
                match backup_task.run_once().await {
                    Ok(event) => {
                        log::info!("task successed: {:?}", task.task_id());
                        task_mgr
                            .task_event_sender()
                            .send(event.clone())
                            .await
                            .expect("todo: channel overflow");

                        match event {
                            BackupTaskEvent::New(_) => assert!(false),
                            BackupTaskEvent::Idle(_) => break,
                            BackupTaskEvent::ErrorAndWillRetry(_) => {}
                            BackupTaskEvent::Fail(_) => break,
                            BackupTaskEvent::Successed(_) => break,
                        }
                        break;
                    }
                    Err(err) => {
                        log::info!("task stopped: {:?}", backup_task.task_id());
                        break;
                    }
                }
            }
        })
    }

    fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
}

pub struct RestoreTask {}

// impl Task for RestoreTask {}