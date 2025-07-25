use super::object_map::{ObjectMap, ObjectMapBody};
use super::storage::{ObjectMapInnerStorage, ObjectMapStorageType};
use super::storage_factory::{ObjectMapStorageOpenMode, GLOBAL_OBJECT_MAP_STORAGE_FACTORY};
use crate::coll::CollectionStorageMode;
use crate::object_map::storage;
use crate::{Base32Codec, HashMethod, NdnError, NdnResult, ObjId};

pub struct ObjectMapBuilder {
    hash_method: HashMethod,
    memory_mode: bool, // If true, use memory storage on simple mode, otherwise use file storage(json or sqlite file)
    storage: Box<dyn ObjectMapInnerStorage>,
}


impl ObjectMapBuilder {
    pub async fn new(
        hash_method: HashMethod,
        coll_mode: Option<CollectionStorageMode>,
        memory_mode: bool, // If true, use memory storage on simple mode, otherwise use file storage(json or sqlite file)
    ) -> NdnResult<Self> {
        let storage_type = ObjectMapStorageType::select_storage_type(coll_mode, memory_mode);

        let mut storage = if storage_type.is_memory() {
            GLOBAL_OBJECT_MAP_STORAGE_FACTORY
                .get()
                .unwrap()
                .open_memory(None, false)?
        } else {
            GLOBAL_OBJECT_MAP_STORAGE_FACTORY
                .get()
                .unwrap()
                .open(
                    None,
                    false,
                    storage_type,
                    ObjectMapStorageOpenMode::CreateNew,
                )
                .await
                .map_err(|e| {
                    let msg = format!("Error opening object map storage: {}", e);
                    error!("{}", msg);
                    e
                })?
        };

        Ok(Self {
            hash_method,
            memory_mode,
            storage,
        })
    }

    pub async fn new_simple() -> NdnResult<Self> {
        Self::new(HashMethod::default(), Some(CollectionStorageMode::Simple), true).await
    }

    pub async fn new_normal() -> NdnResult<Self> {
        Self::new(HashMethod::default(), Some(CollectionStorageMode::Normal), true).await
    }

    pub async fn open(obj_data: serde_json::Value) -> NdnResult<Self> {
        let body: ObjectMapBody = serde_json::from_value(obj_data).map_err(|e| {
            let msg = format!("Error decoding object map body: {}", e);
            error!("{}", msg);
            NdnError::InvalidData(msg)
        })?;

        let (obj_id, _) = body.calc_obj_id();

        let storage_type = body.get_storage_type();
        let storage = match storage_type {
            ObjectMapStorageType::Memory => {
                // If the storage type is memory, we can use the memory storage directly
                GLOBAL_OBJECT_MAP_STORAGE_FACTORY
                    .get()
                    .unwrap()
                    .open_memory(body.content, false)?
            }
            _ => {
                GLOBAL_OBJECT_MAP_STORAGE_FACTORY
                    .get()
                    .unwrap()
                    .open(
                        Some(&obj_id),
                        false,
                        body.get_storage_type(),
                        ObjectMapStorageOpenMode::OpenExisting,
                    )
                    .await?
            }
        };

        Ok(Self {
            hash_method: body.hash_method,
            memory_mode: storage_type.is_memory(),
            storage,
        })
    }

    // Always clone the storage for modify
    // This is to ensure that the original object map file is not modified
    pub async fn from_object_map(object_map: &ObjectMap) -> NdnResult<Self> {
        let storage = object_map.clone_storage_for_modify().await?;
        Ok(Self {
            hash_method: object_map.hash_method(),
            memory_mode: storage.get_type().is_memory(),
            storage,
        })
    }

    pub fn with_memory_mode(mut self, memory_mode: bool) -> Self {
        self.memory_mode = memory_mode;
        self
    }

    // Get the storage type of current using storage, maybe changed after build
    pub fn storage_type(&self) -> ObjectMapStorageType {
        self.storage.get_type()
    }

    pub fn put_object(&mut self, key: &str, obj_id: &ObjId) -> NdnResult<()> {
        self.storage.put(&key, &obj_id)
    }

    pub fn get_object(&self, key: &str) -> NdnResult<Option<ObjId>> {
        let ret = self.storage.get(key)?;
        if ret.is_none() {
            return Ok(None);
        }

        Ok(Some(ret.unwrap().0))
    }

    // Try to remove the object from the map, return the object id
    pub fn remove_object(&mut self, key: &str) -> NdnResult<Option<ObjId>> {
        self.storage.remove(key)
    }

    pub fn is_object_exist(&self, key: &str) -> NdnResult<bool> {
        self.storage.is_exist(&key)
    }

    pub fn iter<'a>(&'a self) -> Box<dyn Iterator<Item = (String, ObjId, Option<u64>)> + 'a> {
        let iter = self.storage.iter();
        Box::new(iter)
    }

    pub async fn build(mut self) -> NdnResult<ObjectMap> {
        let mtree =
            ObjectMap::regenerate_merkle_tree(&mut self.storage, self.hash_method, false).await?;

        let root_hash = mtree.get_root_hash();
        let root_hash_str = Base32Codec::to_base32(&root_hash);
        let total_count = self.storage.stat()?.total_count;

        let mut body = ObjectMapBody {
            hash_method: self.hash_method.clone(),
            root_hash: root_hash_str,
            total_count,
            content: None,
        };

        let obj_id = body.calc_obj_id().0;

        // Check if the collection storage mode is matched
        let storage_mode = CollectionStorageMode::select_mode(Some(total_count));
        let storage_type =
            ObjectMapStorageType::select_storage_type(Some(storage_mode), self.memory_mode);
        let storage = if self.storage.get_type() != storage_type {
            GLOBAL_OBJECT_MAP_STORAGE_FACTORY
                .get()
                .unwrap()
                .switch_storage(&obj_id, self.storage, storage_type)
                .await?
        } else {
            // If the storage type is matched, we can continue to use the current storage
            // Save the object map to storage
            GLOBAL_OBJECT_MAP_STORAGE_FACTORY
                .get()
                .unwrap()
                .save(&obj_id, &mut *self.storage)
                .await
                .map_err(|e| {
                    let msg = format!("Error saving object map: {}", e);
                    error!("{}", msg);
                    e
                })?;

            self.storage
        };

        assert_eq!(
            storage.get_type(),
            storage_type,
            "Storage type mismatch after switching: {:?} != {:?}",
            storage.get_type(),
            storage_type
        );

        if storage_type.is_memory() {
            let content = storage.dump().await?;
            body.content = content;
        }

        let object_map = ObjectMap::new(obj_id, body, storage, mtree);
        Ok(object_map)
    }
}
