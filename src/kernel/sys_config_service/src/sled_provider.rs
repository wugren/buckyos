use async_trait::async_trait;
use sled::{Db, IVec};
use std::{collections::HashMap, sync::Arc};
use crate::kv_provider::*;
use log::*;
use buckyos_kit::*;
pub struct SledStore {
    db: Arc<Db>,
}


impl SledStore {
    pub fn new() -> std::result::Result<Self, Box<dyn std::error::Error>> {
        let root_path  = get_buckyos_root_dir();
        let path = root_path.join("data").join("sys_config");
        
        let db = sled::open(path)?;
        Ok(SledStore { db: Arc::new(db) })
    }
}

#[async_trait]
impl KVStoreProvider for SledStore {
    async fn get(&self, key: String) -> Result< Option<String> > {
        match self.db.get(key.clone()).map_err(|error| KVStoreErrors::InternalError(error.to_string()))? {
            Some(value) => {
                let result = String::from_utf8(value.to_vec())
                    .map_err(|_err| KVStoreErrors::InternalError("Invalid UTF-8 sequence".to_string()))?;
                info!("Sled Get key:[{}] value length:[{}]", key, result.len());
                Ok(Some(result))
            },
            None => Ok(None)
        }
    }

    async fn set(&self, key: String, value: String) -> Result<()> {
        self.db.insert(key.clone(), value.clone().into_bytes())
            .map_err(|err| KVStoreErrors::InternalError(err.to_string()))?;
        self.db.flush().map_err(|err| KVStoreErrors::InternalError(err.to_string()))?;
        info!("Sled Set key:[{}] to value:[{}]", key, value);
        Ok(())
    }

    async fn create(&self, key: &str, value: &str) -> Result<()> {
        let create_result =  self.db.compare_and_swap(key.to_string(), 
            None as Option<IVec>,Some(value.to_string().into_bytes()))
            .map_err(|err| KVStoreErrors::InternalError(err.to_string()));

        match create_result {
            Ok(Ok(_)) => {
                self.db.flush().map_err(|err| KVStoreErrors::InternalError(err.to_string()))?;
                info!("Sled Create key:[{}] to value:[{}]", key, value);
                return Ok(())
            },
            Ok(Err(_)) => {
                warn!("Sled Create key:[{}] to value:[{}] failed, key already exist", key, value);
                return Err(KVStoreErrors::KeyExist(key.to_string()));
            },
            Err(err) => {
                return Err(KVStoreErrors::InternalError(err.to_string()));
            }
        } 
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let result = self.db.remove(key.to_string())
            .map_err(|err| KVStoreErrors::InternalError(err.to_string()))?;
        self.db.flush().map_err(|err| KVStoreErrors::InternalError(err.to_string()))?;
        if result.is_none() {
            return Err(KVStoreErrors::KeyNotFound(key.to_string()));
        }
        info!("Sled Delete key:[{}]", key);
        Ok(())
    }

    async fn list_data(&self,key_perfix:&str) -> Result<HashMap<String,String>> {
        let mut result = HashMap::new();
        let iter = self.db.scan_prefix(key_perfix.to_string());
        for item in iter {
            if item.is_ok() {
                let (key,value) = item.unwrap();
                let key_str = String::from_utf8(key.to_vec())
                    .map_err(|err| KVStoreErrors::InternalError(err.to_string()))?;
                let value_str = String::from_utf8(value.to_vec())
                    .map_err(|err| KVStoreErrors::InternalError(err.to_string()))?;
                result.insert(key_str,value_str);
            }
        }
        Ok(result)
    }

    async fn list_keys(&self, key_prefix: &str) -> Result<Vec<String>> {
        let mut result = Vec::new();
        let iter = self.db.scan_prefix(key_prefix.to_string()).keys();
        for key in iter {
            if let Ok(key) = key {
                if let Ok(key_str) = String::from_utf8(key.to_vec()) {
                    result.push(key_str);
                }
            }
        }
        Ok(result)
    }

    async fn list_direct_children(&self, prefix: String) -> Result<Vec<String>> {
        let mut result = Vec::new();
        let prefix = prefix.trim_end_matches('/').to_string();
        
        // 计算范围的起始和结束
        let start = format!("{}/", prefix);
        let end = format!("{}/\u{FFFF}", prefix); // \u{FFFF} 是一个很大的 Unicode 字符
        
        for item in self.db.range(start.as_bytes()..end.as_bytes()) {
            let (key, _) = item.map_err(|err| KVStoreErrors::InternalError(err.to_string()))?;
            let key_str = String::from_utf8_lossy(&key).to_string();
            

            if !key_str[prefix.len()+1..].contains('/') {
                result.push(key_str);
            }
        }

        Ok(result)
    }
}

