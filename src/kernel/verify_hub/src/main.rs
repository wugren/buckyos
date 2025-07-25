#[allow(unused_braces)]
use base64::{engine::general_purpose::STANDARD, Engine as _};
use lazy_static::lazy_static;
use log::*;
use rand::prelude::*;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use warp::Filter;

use ::kRPC::*;
use buckyos_api::*;
use buckyos_kit::*;
use jsonwebtoken::{DecodingKey, EncodingKey, Validation};
use name_lib::*;

type Result<T> = std::result::Result<T, RPCErrors>;
enum LoginType {
    ByPassword,
    ByJWT,
}

#[derive(Clone, Debug, PartialEq)]
struct VerifyServiceConfig {
    zone_config: ZoneConfig,
    device_id: String,
}

lazy_static! {
    static ref VERIFY_HUB_PRIVATE_KEY: Arc<RwLock<EncodingKey>> = {
        let private_key_pem = r#"
-----BEGIN PRIVATE KEY-----
MC4CAQAwBQYDK2VwBCIEIMDp9endjUnT2o4ImedpgvhVFyZEunZqG+ca0mka8oRp
-----END PRIVATE KEY-----
"#;
        let private_key = EncodingKey::from_ed_pem(private_key_pem.as_bytes()).unwrap();
        Arc::new(RwLock::new(private_key))
    };
    static ref TOKEN_CACHE: Arc<Mutex<HashMap<String, RPCSessionToken>>> =
        Arc::new(Mutex::new(HashMap::new()));
    static ref TRUSTKEY_CACHE: Arc<Mutex<HashMap<String, DecodingKey>>> =
        Arc::new(Mutex::new(HashMap::new()));
    static ref VERIFY_SERVICE_CONFIG: Arc<Mutex<Option<VerifyServiceConfig>>> =
        Arc::new(Mutex::new(None));
    static ref MY_RPC_TOKEN: Arc<Mutex<Option<RPCSessionToken>>> =  Arc::new(Mutex::new(None)) ;
}

async fn generate_session_token(
    appid: &str,
    userid: &str,
    nonce: u64,
    session: u64,
    duration: u64,
) -> RPCSessionToken {
    let now = buckyos_get_unix_timestamp();
    let exp = now + duration;

    let mut session_token = RPCSessionToken {
        token_type: RPCSessionTokenType::JWT,
        nonce: Some(nonce),
        appid: Some(appid.to_string()),
        userid: Some(userid.to_string()),
        token: None,
        session: Some(session),
        iss: Some("verify-hub".to_string()),
        exp: Some(exp),
    };

    {
        let private_key = VERIFY_HUB_PRIVATE_KEY.read().await;
        session_token.token = Some(
            session_token
                .generate_jwt(Some("verify-hub".to_string()), &private_key)
                .unwrap(),
        );
    }

    return session_token;
}

async fn get_my_krpc_token() -> RPCSessionToken {
    let now = buckyos_get_unix_timestamp();
    let device_id = VERIFY_SERVICE_CONFIG
        .lock()
        .await
        .as_ref()
        .unwrap()
        .device_id
        .clone();

    let my_rpc_token = MY_RPC_TOKEN.lock().await;
    if my_rpc_token.is_some() {
        let token = my_rpc_token.as_ref().unwrap();
        if token.exp.is_some() {
            if token.exp.unwrap() - 30 > now {
                return token.clone();
            }
        }
    }
    drop(my_rpc_token);

    let exp = now + VERIFY_HUB_TOKEN_EXPIRE_TIME;

    let mut session_token = RPCSessionToken {
        token_type: RPCSessionTokenType::JWT,
        nonce: None,
        appid: Some("verify-hub".to_string()),
        userid: Some(device_id),
        token: None,
        session: None,
        iss: Some("verify-hub".to_string()),
        exp: Some(exp),
    };

    {
        let private_key = VERIFY_HUB_PRIVATE_KEY.read().await;
        session_token.token = Some(
            session_token
                .generate_jwt(Some("verify-hub".to_string()), &private_key)
                .unwrap(),
        );
    }

    let mut my_rpc_token = MY_RPC_TOKEN.lock().await;
    *my_rpc_token = Some(session_token.clone());
    return session_token;
}

async fn load_token_from_cache(key: &str) -> Option<RPCSessionToken> {
    let cache = TOKEN_CACHE.lock().await;
    let token = cache.get(key);
    if token.is_none() {
        return None;
    } else {
        return Some(token.unwrap().clone());
    }
}

async fn cache_token(key: &str, token: RPCSessionToken) {
    TOKEN_CACHE.lock().await.insert(key.to_string(), token);
}

async fn load_trustkey_from_cache(kid: &str) -> Option<DecodingKey> {
    let cache = TRUSTKEY_CACHE.lock().await;
    let decoding_key = cache.get(kid);
    if decoding_key.is_none() {
        return None;
    }
    return Some(decoding_key.unwrap().clone());
}

async fn cache_trustkey(kid: &str, key: DecodingKey) {
    TRUSTKEY_CACHE.lock().await.insert(kid.to_string(), key);
}

async fn get_trust_public_key_from_kid(kid: &Option<String>) -> Result<DecodingKey> {
    //turst keys include : zone's owner, admin users, server device
    //kid : {owner}
    //kid : #device_id

    let kid = kid.clone().unwrap_or("verify-hub".to_string());
    let cached_key = load_trustkey_from_cache(&kid).await;
    if cached_key.is_some() {
        return Ok(cached_key.unwrap());
    }

    //not found in trustkey_cache, try load from system config service
    let result_key: DecodingKey;
    if kid == "root" {
        //load zone config from system config service
        let owner_auth_key = VERIFY_SERVICE_CONFIG
            .lock()
            .await
            .as_ref()
            .unwrap()
            .zone_config
            .get_auth_key(None)
            .ok_or(RPCErrors::ReasonError(
                "Owner public key not found".to_string(),
            ))?;
        result_key = owner_auth_key.0;
        info!("load owner public key from zone config");
    } else {
        //load device config from system config service(not from name-lib)
        let _zone_config = VERIFY_SERVICE_CONFIG
            .lock()
            .await
            .as_ref()
            .unwrap()
            .zone_config
            .clone();
        let rpc_token = get_my_krpc_token().await;
        let rpc_token_str = rpc_token.to_string();
        let system_config_client = SystemConfigClient::new(None, Some(rpc_token_str.as_str()));
        let control_panel_client = ControlPanelClient::new(system_config_client);
        let device_config = control_panel_client.get_device_config(&kid).await;
        if device_config.is_err() {
            warn!(
                "load device {} config from system config service failed",
                kid
            );
            return Err(RPCErrors::ReasonError(
                "Device config not found".to_string(),
            ));
        }
        let device_config = device_config.unwrap();
        let result_device_key = device_config
            .get_auth_key(None)
            .ok_or(RPCErrors::ReasonError(
                "Device public key not found".to_string(),
            ))?;
        result_key = result_device_key.0;
    }

    //kid is device_id,try load device config from system config service
    cache_trustkey(&kid, result_key.clone()).await;

    return Ok(result_key);
}

//return (kid,payload)
async fn verify_jwt(jwt: &str) -> Result<(String, Value)> {
    let header: jsonwebtoken::Header = jsonwebtoken::decode_header(jwt).map_err(|error| {
        error!("JWT decode header error: {}", error);
        RPCErrors::ReasonError("JWT decode header error".to_string())
    })?;
    let validation = Validation::new(header.alg);

    //try get public key from header.kid
    let public_key = get_trust_public_key_from_kid(&header.kid).await?;

    //verify jwt
    let decoded_token =
        jsonwebtoken::decode::<Value>(&jwt, &public_key, &validation).map_err(|error| {
            error!("JWT verify error: {}", error);
            RPCErrors::ReasonError("JWT verify error".to_string())
        })?;

    let kid = header.kid.unwrap_or("verify-hub".to_string());
    return Ok((kid, decoded_token.claims));
}

// other service can use this api to verify session token which is issued by verify-hub
async fn handle_verify_session_token(params: Value) -> Result<Value> {
    let session_token = params
        .get("session_token")
        .ok_or(RPCErrors::ReasonError("Missing session_token".to_string()))?;
    let session_token = session_token
        .as_str()
        .ok_or(RPCErrors::ReasonError("Invalid session_token".to_string()))?;
    let first_dot = session_token.find('.');
    if first_dot.is_none() {
        //this is not a jwt token, use token-store to verify
        return Err(RPCErrors::InvalidToken("not a jwt token".to_string()));
    } else {
        //this is a jwt token, verify it locally
        let (_iss, json_body) = verify_jwt(session_token).await?;
        let rpc_session_token: RPCSessionToken = serde_json::from_value(json_body.clone())
            .map_err(|error| RPCErrors::ReasonError(error.to_string()))?;
        let now = buckyos_get_unix_timestamp();
        if rpc_session_token.exp.is_none() {
            return Err(RPCErrors::ReasonError("Token expired".to_string()));
        }
        let exp = rpc_session_token.exp.unwrap();
        if now > exp {
            return Err(RPCErrors::ReasonError("Token expired".to_string()));
        }
        Ok(json_body)
    }
}

async fn handle_login_by_jwt(params: Value, _login_nonce: u64) -> Result<RPCSessionToken> {
    let jwt = params
        .get("jwt")
        .ok_or(RPCErrors::ReasonError("Missing jwt".to_string()))?;
    let jwt = jwt
        .as_str()
        .ok_or(RPCErrors::ReasonError("Invalid jwt".to_string()))?;
    let (iss_kid, jwt_payload) = verify_jwt(jwt).await?;

    let userid = jwt_payload
        .get("userid")
        .ok_or(RPCErrors::ReasonError("Missing userid".to_string()))?;
    let userid = userid
        .as_str()
        .ok_or(RPCErrors::ReasonError("Invalid userid".to_string()))?;
    let appid = jwt_payload
        .get("appid")
        .ok_or(RPCErrors::ReasonError("Missing appid".to_string()))?;
    let appid = appid
        .as_str()
        .ok_or(RPCErrors::ReasonError("Invalid appid".to_string()))?;

    let exp = jwt_payload
        .get("exp")
        .ok_or(RPCErrors::ReasonError("Missing exp".to_string()))?;
    let exp = exp
        .as_u64()
        .ok_or(RPCErrors::ReasonError("Invalid exp".to_string()))?;

    let login_nonce = jwt_payload.get("nonce");
    let mut token_nonce: u64 = 0;
    if login_nonce.is_some() {
        token_nonce = login_nonce
            .unwrap()
            .as_u64()
            .ok_or(RPCErrors::ReasonError("Invalid login_nonce".to_string()))?;
    }
    let next_nonce;
    {
        let mut rng = rand::rng();
        next_nonce = rng.random::<u64>();
        drop(rng)
    }

    match iss_kid.as_str() {
        "verify-hub" => {
            //verify-hub's jwt
            let session_id = jwt_payload.get("session");
            if session_id.is_none() {
                return Err(RPCErrors::ReasonError("Missing session_id".to_string()));
            }
            let session_id = session_id
                .unwrap()
                .as_u64()
                .ok_or(RPCErrors::ReasonError("Invalid session_id".to_string()))?;
            if session_id == 0 {
                return Err(RPCErrors::ReasonError("Invalid session_id".to_string()));
            }
            let session_key = format!("{}_{}_{}", userid, appid, session_id);
            info!("handle refresh token by jwt for session:{}", session_key);
            let cache_result = load_token_from_cache(session_key.as_str()).await;
            if cache_result.is_none() {
                warn!("Session not found for session:{}", session_key);
                return Err(RPCErrors::ReasonError(
                    "Session token not found".to_string(),
                ));
            }
            let old_token = cache_result.unwrap();
            if old_token.nonce.unwrap() != token_nonce {
                warn!(
                    "Invalid nonce (session_nonce), old_token:{:?} req.token.nonce:{}",
                    old_token, token_nonce
                );
                return Err(RPCErrors::ReasonError(
                    "Invalid nonce (session_nonce)".to_string(),
                ));
            }
            let session_token = generate_session_token(
                appid,
                userid,
                next_nonce,
                session_id,
                VERIFY_HUB_TOKEN_EXPIRE_TIME,
            )
            .await;
            //store session token to cache
            cache_token(session_key.as_str(), session_token.clone()).await; //other service's jwt
            info!("refresh token success:{}", session_key);
            return Ok(session_token);
        }
        _ => {
            info!("handle login by jwt");
            let session_key = format!("{}_{}_{}", userid, appid, token_nonce);

            if buckyos_get_unix_timestamp() > exp {
                return Err(RPCErrors::ReasonError("Token expired".to_string()));
            }

            //load last login token from cache （TODO: from db?)
            let cache_result = load_token_from_cache(session_key.as_str()).await;
            if cache_result.is_some() {
                return Err(RPCErrors::ReasonError("login jwt already used".to_string()));
            }

            let session_token = generate_session_token(
                appid,
                userid,
                next_nonce,
                token_nonce,
                VERIFY_HUB_TOKEN_EXPIRE_TIME,
            )
            .await;
            //store session token to cache
            cache_token(session_key.as_str(), session_token.clone()).await;
            info!(
                "login success, generate session token for user:{},new session_id:{},next_nonce:{}",
                userid, session_key, next_nonce
            );
            return Ok(session_token);
        }
    }
}

//login by username + password
async fn handle_login_by_password(params: Value, login_nonce: u64) -> Result<Value> {
    let password = params
        .get("password")
        .ok_or(RPCErrors::ParseRequestError("Missing password".to_string()))?;
    let password = password
        .as_str()
        .ok_or(RPCErrors::ReasonError("Invalid password".to_string()))?;
    let username = params
        .get("username")
        .ok_or(RPCErrors::ParseRequestError("Missing username".to_string()))?;
    let username = username
        .as_str()
        .ok_or(RPCErrors::ReasonError("Invalid username".to_string()))?;
    let appid = params
        .get("appid")
        .ok_or(RPCErrors::ParseRequestError("Missing appid".to_string()))?;
    let appid = appid
        .as_str()
        .ok_or(RPCErrors::ReasonError("Invalid appid".to_string()))?;

    // TODO: verify appid matches the target domain
    // The logic for verifying that appid matches the target domain is an operational logic,
    //lanned to be placed in the cyfs-gatewayp configuration file for easy adjustment through configuration

    let now = buckyos_get_unix_timestamp() * 1000;
    let abs_diff = now.abs_diff(login_nonce);
    debug!(
        "{} login nonce and now abs_diff:{},from:{}",
        username, abs_diff, appid
    );
    if now.abs_diff(login_nonce) > 3600 * 1000 * 8 {
        warn!(
            "{} login nonce is too old,abs_diff:{},this is a possible ATTACK?",
            username, abs_diff
        );
        return Err(RPCErrors::ParseRequestError("Invalid nonce".to_string()));
    }

    //read account info from system config service
    let user_info_path = format!("users/{}/settings", username);
    let rpc_token = get_my_krpc_token().await;
    let rpc_token_str = rpc_token.to_string();
    let system_config_client = SystemConfigClient::new(None, Some(rpc_token_str.as_str()));
    let user_info_result = system_config_client.get(user_info_path.as_str()).await;
    if user_info_result.is_err() {
        warn!(
            "handle_login_by_password: user not found {}",
            user_info_path
        );
        return Err(RPCErrors::UserNotFound(username.to_string()));
    }
    let user_info = user_info_result.unwrap().value;
    let user_info: serde_json::Value = serde_json::from_str(&user_info)
        .map_err(|error| RPCErrors::ReasonError(error.to_string()))?;
    let store_password = user_info.get("password").ok_or(RPCErrors::ReasonError(
        "password not set,cann't login by password".to_string(),
    ))?;
    let store_password = store_password
        .as_str()
        .ok_or(RPCErrors::ReasonError("Invalid password".to_string()))?;
    let user_type = user_info.get("type").ok_or(RPCErrors::ReasonError(
        "user type not set,cann't login by password".to_string(),
    ))?;
    let user_type = user_type
        .as_str()
        .ok_or(RPCErrors::ReasonError("Invalid user type".to_string()))?;

    //encode password with nonce and check it is right
    let password_hash_input = STANDARD
        .decode(password)
        .map_err(|error| RPCErrors::ReasonError(error.to_string()))?;

    let salt = format!("{}{}", store_password, login_nonce);
    let hash = Sha256::digest(salt.clone()).to_vec();
    if hash != password_hash_input {
        warn!("{} login by password failed,password is wrong! stored_password:{},salt:{},input_password:{},real_input:{:?},my_hash:{:?}",username,store_password,salt,password,password_hash_input,hash);
        return Err(RPCErrors::InvalidPassword);
    }

    //generate session token
    info!(
        "login success, generate session token for user:{}",
        username
    );
    let session_id;
    {
        let mut rng = rand::rng();
        session_id = rng.random::<u64>();
    }

    let session_token =
        generate_session_token(appid, username, login_nonce, session_id, 3600 * 24 * 7).await;
    let result_account_info = json!({
        "user_name": username,
        "user_id": username,
        "user_type": user_type,
        "session_token": session_token.to_string()
    });
    return Ok(result_account_info);
}

// async fn handle_query_userid(params:Value) -> Result<Value> {
//     let username = params.get("username")
//         .ok_or(RPCErrors::ReasonError("Missing uername".to_string()))?;
//     let username = username.as_str().ok_or(RPCErrors::ReasonError("Invalid uername".to_string()))?;

//     let user_info_path = format!("users/{}/settings",username);
//     let rpc_token = get_my_krpc_token().await;
//     let rpc_token_str = rpc_token.to_string();
//     let system_config_client = SystemConfigClient::new(None,Some(rpc_token_str.as_str()));
//     let user_info_result = system_config_client.get(user_info_path.as_str()).await;
//     if user_info_result.is_ok() {
//         let (user_info,_version) = user_info_result.unwrap();
//         let user_info:serde_json::Value = serde_json::from_str(&user_info)
//             .map_err(|error| RPCErrors::ReasonError(error.to_string()))?;
//         let this_username = user_info.get("username");
//         if this_username.is_some() {
//             let this_username = this_username.unwrap().as_str().ok_or(RPCErrors::ReasonError("Invalid username".to_string()))?;
//             if this_username == username {
//                 return Ok(json!({
//                     "userid": username
//                 }));
//             }
//         }
//     }

//     Err(RPCErrors::UserNotFound(username.to_string()))
// }

// async fn handle_login_by_signature(params:Value,login_nonce:u64) -> Result<RPCSessionToken> {
//     let userid = params.get("userid")
//     .ok_or(RPCErrors::ReasonError("Missing userid".to_string()))?;
//     let userid = userid.as_str().ok_or(RPCErrors::ReasonError("Invalid userid".to_string()))?;
//     let appid = params.get("appid")
//         .ok_or(RPCErrors::ReasonError("Missing appid".to_string()))?;
//     let appid = appid.as_str().ok_or(RPCErrors::ReasonError("Invalid appid".to_string()))?;

//     let _from = params.get("from")
//     .ok_or(RPCErrors::ReasonError("Missing from".to_string()))?;
//     let from = _from.as_str().ok_or(RPCErrors::ReasonError("Invalid from".to_string()))?;
//     let _signature = params.get("signature")
//         .ok_or(RPCErrors::ReasonError("Missing signature".to_string()))?;
//     let signature = _signature.as_str().ok_or(RPCErrors::ReasonError("Invalid signature".to_string()))?;

//     //verify signature
//     let trust_did_document = get_trust_did_document(from).await?;
//     let device_public_key = trust_did_document.get_auth_key().ok_or(RPCErrors::ReasonError("Device public key not found".to_string()))?;

//     //TODO:check login_nonce > last_login_nonce

//     let mut will_hash = params.clone();
//     let will_hash_obj = will_hash.as_object_mut().unwrap();
//     will_hash_obj.remove("signature");
//     will_hash_obj.remove("type");
//     will_hash_obj.insert(String::from("login_nonce"),json!(login_nonce));

//     if !verify_jws_signature(&will_hash,signature,&device_public_key) {
//         return Err(RPCErrors::ReasonError("Invalid signature".to_string()));
//     }

//     //generate session token
//     let session_token = generate_session_token(appid,userid,login_nonce);

//     return Ok(session_token);
// }

async fn handle_login(params: Value, login_nonce: u64) -> Result<Value> {
    let mut real_login_type = LoginType::ByJWT;
    let login_type = params.get("type");

    if login_type.is_some() {
        let login_type = login_type
            .unwrap()
            .as_str()
            .ok_or(RPCErrors::ReasonError("Invalid login type".to_string()))?;
        match login_type {
            "password" => {
                real_login_type = LoginType::ByPassword;
            }
            "jwt" => {
                real_login_type = LoginType::ByJWT;
            }
            _ => {
                return Err(RPCErrors::ReasonError("Invalid login type".to_string()));
            }
        }
    }

    match real_login_type {
        LoginType::ByJWT => {
            let session_token = handle_login_by_jwt(params, login_nonce).await?;
            return Ok(Value::String(session_token.to_string()));
        }
        LoginType::ByPassword => {
            let account_info = handle_login_by_password(params, login_nonce).await?;
            return Ok(account_info);
        }
    }
}

/**
 curl -X POST http://127.0.0.1/kapi/verify_hub -H "Content-Type: application/json" -d '{"method": "login","params":{"type":"jwt","jwt":"eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJodHRwczovL3d3dy53aGl0ZS5ib3Vjay5pbyIsImF1ZCI6Imh0dHBzOi8vd3d3LndoaXRlLmJvdWNrLmlvIiwiZXhwIjoxNzI3NzIwMDAwLCJpYXQiOjE3Mjc3MTY0MDAsInVzZXJpZCI6ImRpZDpleGFtcGxlOjEyMzQ1Njc4OTAiLCJhcHBpZCI6InN5c3RvbSIsInVzZXJuYW1lIjoiYWxpY2UifQ.6XQ56XQ56XQ56XQ56XQ56XQ56XQ56XQ56XQ56XQ5"}}'
curl -X POST http://127.0.0.1:3300/kapi/verify_hub -H "Content-Type: application/json" -d '{"method": "login","params":{"type":"password","username":"eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJodHRwczovL3d3dy53aGl0ZS5ib3Vjay5pbyIsImF1ZCI6Imh0dHBzOi8vd3d3LndoaXRlLmJvdWNrLmlvIiwiZXhwIjoxNzI3NzIwMDAwLCJpYXQiOjE3Mjc3MTY0MDAsInVzZXJpZCI6ImRpZDpleGFtcGxlOjEyMzQ1Njc4OTAiLCJhcHBpZCI6InN5c3RvbSIsInVzZXJuYW1lIjoiYWxpY2UifQ.6XQ56XQ56XQ56XQ56XQ56XQ56XQ56XQ56XQ56XQ5"}}'
 */
async fn process_request(method: String, param: Value, req_seq: u64) -> ::kRPC::Result<Value> {
    match method.as_str() {
        "login" => {
            return handle_login(param, req_seq).await;
        }
        // "query_userid" => {
        //     return handle_query_userid(param).await;
        // },
        "verify_token" => {
            return handle_verify_session_token(param).await;
        }
        // Add more methods here
        _ => Err(RPCErrors::UnknownMethod(String::from(method))),
    }
}

async fn load_service_config() -> Result<()> {
    info!("start load config from system config service.");
    let session_token = env::var("VERIFY_HUB_SESSION_TOKEN")
        .map_err(|error| RPCErrors::ReasonError(error.to_string()))?;
    let device_rpc_token = RPCSessionToken::from_string(session_token.as_str())?;
    let device_id = device_rpc_token
        .userid
        .ok_or(RPCErrors::ReasonError("device id not found".to_string()))?;
    info!("This device_id:{}", device_id);

    let system_config_client = SystemConfigClient::new(None, Some(session_token.as_str()));

    //load verify-hub private key from system config service
    let private_key_str = system_config_client.get("system/verify-hub/key").await;
    if private_key_str.is_ok() {
        let private_key = private_key_str.unwrap().value;
        let private_key = EncodingKey::from_ed_pem(private_key.as_bytes());
        if private_key.is_ok() {
            let private_key = private_key.unwrap();
            let mut verify_hub_private_key = VERIFY_HUB_PRIVATE_KEY.write().await;
            *verify_hub_private_key = private_key;
        } else {
            warn!("verify_hub private key format error!");
            return Err(RPCErrors::ReasonError(
                "verify_hub private key format error".to_string(),
            ));
        }
    } else {
        warn!("verify_hub private key cann't load from system config service!");
        return Err(RPCErrors::ReasonError(
            "verify_hub private key cann't load from system config service".to_string(),
        ));
    }
    info!("verify_hub private key loaded from system config service OK!");

    let control_panel_client = ControlPanelClient::new(system_config_client);
    let zone_config = control_panel_client.load_zone_config().await;
    if zone_config.is_err() {
        warn!(
            "zone config cann't load from system config service,use default zone config for test!"
        );
        return Err(RPCErrors::ReasonError(
            "zone config cann't load from system config service".to_string(),
        ));
    }
    let zone_config = zone_config.unwrap();
    if zone_config.verify_hub_info.is_none() {
        warn!("zone config verify_hub_info not found!");
        return Err(RPCErrors::ReasonError(
            "zone config verify_hub_info not found".to_string(),
        ));
    }
    let verify_hub_info = zone_config.verify_hub_info.as_ref().unwrap();
    let verify_hub_pub_key = DecodingKey::from_jwk(&verify_hub_info.public_key)
        .map_err(|error| RPCErrors::ReasonError(error.to_string()))?;
    cache_trustkey("verify-hub", verify_hub_pub_key).await;
    info!("verify_hub public key loaded from system config service OK!");

    let new_service_config = VerifyServiceConfig {
        zone_config: zone_config,
        device_id: device_id,
    };

    {
        let mut service_config = VERIFY_SERVICE_CONFIG.lock().await;
        if service_config.is_some() {
            return Ok(());
        }
        service_config.replace(new_service_config);
    }

    info!("verify_hub load_service_config success!");
    Ok(())
}

async fn service_main() -> i32 {
    init_logging("verify_hub", true);
    info!("Starting verify_hub service...");
    //init service config from system config service and env
    let _ = load_service_config().await.map_err(|error| {
        error!("load service config failed:{}", error);
        return -1;
    });
    //load cache from service_cache@dfs:// and service_local_cache@fs://

    let cors_response = warp::path!("kapi" / "verify-hub")
        .and(warp::options())
        .map(|| {
            info!("Handling OPTIONS request");
            warp::http::Response::builder()
                .header("Access-Control-Allow-Origin", "*")
                .header("Access-Control-Allow-Methods", "POST, OPTIONS")
                .header("Access-Control-Allow-Headers", "Content-Type")
                .body("")
        });

    let rpc_route = warp::path!("kapi" / "verify-hub")
        .and(warp::post())
        .and(warp::body::json())
        .and_then(|req: RPCRequest| async move {
            info!(
                "|==>Received request: {}",
                serde_json::to_string(&req).unwrap()
            );
            let process_result = process_request(req.method, req.params, req.id).await;
            let rpc_response: RPCResponse;
            match process_result {
                Ok(result) => {
                    rpc_response = RPCResponse {
                        result: RPCResult::Success(result),
                        seq: req.id,
                        token: None,
                        trace_id: req.trace_id,
                    };
                }
                Err(err) => {
                    rpc_response = RPCResponse {
                        result: RPCResult::Failed(err.to_string()),
                        seq: req.id,
                        token: None,
                        trace_id: req.trace_id,
                    };
                }
            }

            info!(
                "<==|Response: {}",
                serde_json::to_string(&rpc_response).unwrap()
            );
            Ok::<_, warp::Rejection>(warp::reply::json(&rpc_response))
        });

    let routes = cors_response.or(rpc_route);

    info!("verify_hub service initialized, running on port 3300");
    warp::serve(routes).run(([127, 0, 0, 1], 3300)).await;
    return 0;
}

#[tokio::main]
async fn main() {
    service_main().await;
}

#[cfg(test)]
mod test {
    use super::*;
    use jsonwebtoken::{Algorithm, Header};
    use serde_json::json;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use tokio::task;
    use tokio::time::sleep;

    //#[tokio::test]
    async fn test_login_and_verify() {
        //let zone_config = ZoneConfig::new_test_config();
        //env::set_var("BUCKYOS_ZONE_CONFIG", serde_json::to_string(&zone_config).unwrap());
        //env::set_var("SESSION_TOKEN", "abcdefg");//for test only

        let server = task::spawn(async {
            service_main().await;
        });

        sleep(Duration::from_millis(100)).await;
        let test_jwk = json!({
            "kty": "OKP",
            "crv": "Ed25519",
            "x": "gubVIszw-u_d5PVTh-oc8CKAhM9C-ne5G_yUK5BDaXc",
        });
        let public_key_jwk: jsonwebtoken::jwk::Jwk = serde_json::from_value(test_jwk).unwrap();
        let test_pk = DecodingKey::from_jwk(&public_key_jwk).unwrap();

        cache_trustkey("verify-hub", test_pk.clone()).await;
        cache_trustkey("root", test_pk).await;
        let test_owner_private_key_pem = r#"
-----BEGIN PRIVATE KEY-----
MC4CAQAwBQYDK2VwBCIEIMDp9endjUnT2o4ImedpgvhVFyZEunZqG+ca0mka8oRp
-----END PRIVATE KEY-----
"#;
        //login test,use trust device JWT
        let private_key = EncodingKey::from_ed_pem(test_owner_private_key_pem.as_bytes()).unwrap();
        let client = kRPC::new("http://127.0.0.1:3300/kapi/verify-hub", None);
        let mut header = Header::new(Algorithm::EdDSA);
        //完整的kid表达应该是 $zoneid#kid 这种形式，为了提高性能做了一点简化
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        header.kid = Some("{owner}".to_string());
        header.typ = None;
        // let login_params = json!({
        //     "userid": "did:example:1234567890",
        //     "appid": "system",
        //     "exp":(now + 3600) as usize
        // });
        // let token = encode(&header, &login_params, &private_key).unwrap();

        let mut rng = rand::rng();
        let session_id = rng.random::<u64>();

        let test_login_token = RPCSessionToken {
            token_type: RPCSessionTokenType::JWT,
            nonce: Some(buckyos_get_unix_timestamp() * 1_000_000),
            appid: Some("kernel".to_string()),
            userid: Some("alice".to_string()),
            token: None,
            session: Some(session_id),
            iss: Some("{owner}".to_string()),
            exp: Some(now + 3600),
        };

        let test_jwt = test_login_token
            .generate_jwt(Some("{owner}".to_string()), &private_key)
            .unwrap();

        let session_token = client
            .call("login", json!( {"type":"jwt","jwt":test_jwt}))
            .await
            .unwrap();
        print!("session_token:{}", session_token);

        //verify token test,use JWT-session-token

        let verify_result = client
            .call("verify_token", json!( {"session_token":session_token}))
            .await
            .unwrap();
        print!("verify result:{}", verify_result);

        //test expired token

        let session_id = rng.random::<u64>();
        let test_login_token = RPCSessionToken {
            token_type: RPCSessionTokenType::JWT,
            nonce: Some((buckyos_get_unix_timestamp() - 10000) * 1_000_000),
            appid: Some("kernel".to_string()),
            userid: Some("alice".to_string()),
            token: None,
            session: Some(session_id),
            iss: Some("{owner}".to_string()),
            exp: Some(now + 3600),
        };

        let test_jwt = test_login_token
            .generate_jwt(Some("{owner}".to_string()), &private_key)
            .unwrap();

        let session_token = client
            .call("login", json!( {"type":"jwt","jwt":test_jwt}))
            .await;
        assert!(session_token.is_err());

        drop(server);
    }
}
