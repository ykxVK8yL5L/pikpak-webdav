use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration,SystemTime, UNIX_EPOCH};
use url::form_urlencoded;
use httpdate;
use hmacsha::HmacSha;
use sha1::{Sha1, Digest};
use hex_literal::hex;
use base64::encode;
use std::str::from_utf8;
use anyhow::{Result, Context};
use bytes::{Buf, BufMut, Bytes, BytesMut};
use dashmap::DashMap;
use futures_util::future::{ready, ok, FutureExt};
use tracing::{debug, error, trace,info};
use dav_server::{
    davpath::DavPath,
    fs::{
        DavDirEntry, DavFile, DavFileSystem, DavMetaData, FsError, FsFuture, FsStream, OpenOptions,
        ReadDirMeta,DavProp
    },
};
use moka::future::{Cache as AuthCache};
use tracing_subscriber::fmt::format;
use crate::cache::Cache;
use reqwest::{
    header::{HeaderMap, HeaderValue},
    StatusCode,
};
use tokio::{
    sync::{oneshot, RwLock},
    time,
};
use serde::de::DeserializeOwned;
use serde::{Serialize,Deserialize};
use quick_xml::de::from_str;
use quick_xml::Writer;
use quick_xml::se::Serializer as XmlSerializer;
use serde_json::json;
use reqwest::header::RANGE;

pub use crate::model::*;


const ORIGIN: &str = "https://api-drive.mypikpak.com/drive/v1/files";
const REFERER: &str = "https://api-drive.mypikpak.com/drive/v1/files";
const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/92.0.4515.131 Safari/537.36";
const UPLOAD_CHUNK_SIZE: u64 = 16 * 1024 * 1024; // 16MB

#[derive(Clone)]
pub struct WebdavDriveFileSystem {
    credentials:Credentials,
    auth_cache:AuthCache<String, String>,
    dir_cache: Cache,
    uploading: Arc<DashMap<String, Vec<WebdavFile>>>,
    root: PathBuf,
    client:reqwest::Client,
    proxy_url:String,
    upload_buffer_size: usize,
    skip_upload_same_size: bool,
    prefer_http_download: bool,
}

impl WebdavDriveFileSystem {
    pub async fn new(
        credentials:Credentials,
        root: String,
        cache_size: u64,
        cache_ttl: u64,
        proxy_url: String,
        upload_buffer_size: usize,
        skip_upload_same_size: bool,
        prefer_http_download: bool,
    ) -> Result<Self> {
        let dir_cache = Cache::new(cache_size, cache_ttl);
        debug!("dir cache initialized");
        let root = if root.starts_with('/') {
            PathBuf::from(root)
        } else {
            Path::new("/").join(root)
        };

        let mut headers = HeaderMap::new();
        headers.insert("Origin", HeaderValue::from_static(ORIGIN));
        headers.insert("Referer", HeaderValue::from_static(REFERER));
        //headers.insert("Referer", HeaderValue::from_static(REFERER));
        let client = reqwest::Client::builder()
            .user_agent(UA)
            .default_headers(headers)
            .pool_idle_timeout(Duration::from_secs(50))
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()?;
        let auth_cache = AuthCache::new(2);

        let driver = Self {
            credentials,
            auth_cache,
            dir_cache,
            proxy_url,
            uploading: Arc::new(DashMap::new()),
            root,
            client,
            upload_buffer_size,
            skip_upload_same_size,
            prefer_http_download,
        };

        if let Err(err) = driver.update_token().await {
            error!(error = %err, "save access token failed");
        }

        driver.dir_cache.invalidate_all();

        Ok(driver)

    }

    async fn update_token(&self)  -> Result<()>{
        let mut data = HashMap::new();
        data.insert("captcha_token", "");
        data.insert("client_id", "YNxT9w7GMdWvEOKa");
        data.insert("client_secret", "dbw2OtmVEeuUvIptb1Coyg");
        data.insert("username", &self.credentials.username);
        data.insert("password", &self.credentials.password);

        let mut rurl = format!("https://user.mypikpak.com/v1/auth/signin");
        if self.proxy_url.len()>4{
            rurl = format!("{}/https://user.mypikpak.com/v1/auth/signin",&self.proxy_url);
        }

       let url = rurl;
       let res = self
            .client
            .post(url)
            .json(&data)
            .send()
            .await?;
        match res.error_for_status_ref() {
            Ok(_) => {
                let res = res.json::<RefreshTokenResponse>().await?;
                let access_token = "access_token".to_string();
                self.auth_cache.insert(access_token, res.access_token).await;
            }
            Err(err) => {
                let msg = res.text().await?;
                let context = format!("{}: {}", err, msg);
                debug!(msg=%msg);
            }
        }
        Ok(())
    }

    async fn request<U>(&self, url: String) -> Result<Option<U>>
    where
        U: DeserializeOwned,
    {
        let access_token_key = "access_token".to_string();
        let access_token = self.auth_cache.get(&access_token_key).unwrap();
        let url = reqwest::Url::parse(&url)?;
        let res = self
            .client
            .get(url.clone())
            .bearer_auth(&access_token)
            .send()
            .await?
            .error_for_status();
        match res {
            Ok(res) => {
                if res.status() == StatusCode::NO_CONTENT {
                    return Ok(None);
                }
                let res = res.json::<U>().await?;
                Ok(Some(res))
            }
            Err(err) => {
                match err.status() {
                    Some(
                        status_code
                        @
                        // 4xx
                        (StatusCode::UNAUTHORIZED
                        | StatusCode::REQUEST_TIMEOUT
                        | StatusCode::TOO_MANY_REQUESTS
                        // 5xx
                        | StatusCode::INTERNAL_SERVER_ERROR
                        | StatusCode::BAD_GATEWAY
                        | StatusCode::SERVICE_UNAVAILABLE
                        | StatusCode::GATEWAY_TIMEOUT),
                    ) => {
                        if status_code == StatusCode::UNAUTHORIZED {
                            // refresh token and retry
                            self.update_token().await;
                        } else {
                            // wait for a while and retry
                            time::sleep(Duration::from_secs(1)).await;
                        }
                        let res = self
                            .client
                            .get(url)
                            .bearer_auth(&access_token)
                            .send()
                            .await?
                            .error_for_status()?;
                        if res.status() == StatusCode::NO_CONTENT {
                            return Ok(None);
                        }
                        let res = res.json::<U>().await?;
                        Ok(Some(res))
                    }
                    _ => Err(err.into()),
                }
            }
        }
    }



    async fn post_request<T, U>(&self, url: String, req: &T) -> Result<Option<U>>
    where
        T: Serialize + ?Sized,
        U: DeserializeOwned,
    {
        let access_token_key = "access_token".to_string();
        let access_token = self.auth_cache.get(&access_token_key).unwrap();
        let url = reqwest::Url::parse(&url)?;
        let res = self
            .client
            .post(url.clone())
            .json(&req)
            .bearer_auth(&access_token)
            .send()
            .await?
            .error_for_status();
        match res {
            Ok(res) => {
                if res.status() == StatusCode::NO_CONTENT {
                    return Ok(None);
                }
                let res = res.json::<U>().await?;
                Ok(Some(res))
            }
            Err(err) => {
                match err.status() {
                    Some(
                        status_code
                        @
                        // 4xx
                        (StatusCode::UNAUTHORIZED
                        | StatusCode::REQUEST_TIMEOUT
                        | StatusCode::TOO_MANY_REQUESTS
                        // 5xx
                        | StatusCode::INTERNAL_SERVER_ERROR
                        | StatusCode::BAD_GATEWAY
                        | StatusCode::SERVICE_UNAVAILABLE
                        | StatusCode::GATEWAY_TIMEOUT),
                    ) => {
                        if status_code == StatusCode::UNAUTHORIZED {
                            // refresh token and retry
                            self.update_token().await;
                        } else {
                            // wait for a while and retry
                            time::sleep(Duration::from_secs(1)).await;
                        }
                        let res = self
                            .client
                            .post(url)
                            .json(&req)
                            .bearer_auth(&access_token)
                            .send()
                            .await?
                            .error_for_status()?;
                        if res.status() == StatusCode::NO_CONTENT {
                            return Ok(None);
                        }
                        let res = res.json::<U>().await?;
                        Ok(Some(res))
                    }
                    _ => Err(err.into()),
                }
            }
        }
    }

    async fn patch_request<T, U>(&self, url: String, req: &T) -> Result<Option<U>>
    where
        T: Serialize + ?Sized,
        U: DeserializeOwned,
    {
        let access_token_key = "access_token".to_string();
        let access_token = self.auth_cache.get(&access_token_key).unwrap();
        let url = reqwest::Url::parse(&url)?;
        let res = self
            .client
            .patch(url.clone())
            .json(&req)
            .bearer_auth(&access_token)
            .send()
            .await?
            .error_for_status();
        match res {
            Ok(res) => {
                if res.status() == StatusCode::NO_CONTENT {
                    return Ok(None);
                }
                let res = res.json::<U>().await?;
                Ok(Some(res))
            }
            Err(err) => {
                match err.status() {
                    Some(
                        status_code
                        @
                        // 4xx
                        (StatusCode::UNAUTHORIZED
                        | StatusCode::REQUEST_TIMEOUT
                        | StatusCode::TOO_MANY_REQUESTS
                        // 5xx
                        | StatusCode::INTERNAL_SERVER_ERROR
                        | StatusCode::BAD_GATEWAY
                        | StatusCode::SERVICE_UNAVAILABLE
                        | StatusCode::GATEWAY_TIMEOUT),
                    ) => {
                        if status_code == StatusCode::UNAUTHORIZED {
                            // refresh token and retry
                            self.update_token().await;
                        } else {
                            // wait for a while and retry
                            time::sleep(Duration::from_secs(1)).await;
                        }
                        let res = self
                            .client
                            .post(url)
                            .json(&req)
                            .bearer_auth(&access_token)
                            .send()
                            .await?
                            .error_for_status()?;
                        if res.status() == StatusCode::NO_CONTENT {
                            return Ok(None);
                        }
                        let res = res.json::<U>().await?;
                        Ok(Some(res))
                    }
                    _ => Err(err.into()),
                }
            }
        }
    }

    async fn create_folder(&self, parent_id:&str, folder_name: &str) -> Result<WebdavFile> {
        let mut rurl = format!("https://api-drive.mypikpak.com/drive/v1/files");
        if self.proxy_url.len()>4{
            rurl = format!("{}/https://api-drive.mypikpak.com/drive/v1/files",&self.proxy_url);
        }
        let url = rurl;
        let req = CreateFolderRequest{kind:"drive#folder",name:folder_name,parent_id:parent_id};

  
        let res:WebdavFile = match  self.post_request(url, &req).await{
            Ok(res)=>res.unwrap(),
            Err(err)=>{
                return Err(err);
            }
        };

        Ok(res)
    }

    pub async fn remove_file(&self, file_id: &str) -> Result<()> {
        //let trashurl = "https://api-drive.mypikpak.com/drive/v1/files:batchTrash"    //放入回收站
        //let deleteurl = "https://api-drive.mypikpak.com/drive/v1/files:batchDelete"   //彻底删除
        let mut rurl = format!("https://api-drive.mypikpak.com/drive/v1/files:batchDelete");
        if self.proxy_url.len()>4{
            rurl = format!("{}/https://api-drive.mypikpak.com/drive/v1/files:batchDelete",&self.proxy_url);
        }
        let url = rurl;
        let req = DelFileRequest{ids:vec![file_id.to_string()]};
        self.post_request(url, &req).await?.context("remove file fail")?;
        Ok(())
    }

    pub async fn rename_file(&self, file_id: &str, new_name: &str) -> Result<()> {
        let mut rurl = format!("https://api-drive.mypikpak.com/drive/v1/files/{}",file_id);
        if self.proxy_url.len()>4{
            rurl = format!("{}/https://api-drive.mypikpak.com/drive/v1/files/{}",&self.proxy_url,file_id);
        }
        let url = rurl;
        let req = RenameFileRequest{name:new_name};
        self.patch_request(url, &req)
        .await?
        .context("expect response")?;
        Ok(())
    }


    pub async fn move_file(&self, file_id: &str, new_parent_id: &str) -> Result<()> {
        let mut rurl = format!("https://api-drive.mypikpak.com/drive/v1/files:batchMove");
        if self.proxy_url.len()>4{
            rurl = format!("{}/https://api-drive.mypikpak.com/drive/v1/files:batchMove",&self.proxy_url);
        }
        let url = rurl;
        let req = MoveFileRequest{ids:vec![file_id.to_string()],to:MoveTo { parent_id: new_parent_id.to_string()}};

        self.post_request(url, &req)
        .await?
        .context("expect response")?;

        Ok(())
    }

    pub async fn copy_file(&self, file_id: &str, new_parent_id: &str) -> Result<()> {
        let mut rurl = format!("https://api-drive.mypikpak.com/drive/v1/files:batchCopy");
        if self.proxy_url.len()>4{
            rurl = format!("{}/https://api-drive.mypikpak.com/drive/v1/files:batchCopy",&self.proxy_url);
        }
        let url = rurl;
        let req = MoveFileRequest{ids:vec![file_id.to_string()],to:MoveTo { parent_id: new_parent_id.to_string()}};

        self.post_request(url, &req)
        .await?
        .context("expect response")?;

        Ok(())
    }

    pub async fn get_useage_quota(&self) -> Result<(u64, u64)> {
        let mut rurl = format!("https://api-drive.mypikpak.com/drive/v1/about");
        if self.proxy_url.len()>4{
            rurl = format!("{}/https://api-drive.mypikpak.com/drive/v1/about",&self.proxy_url);
        }
        let url = rurl;
       
        let res:QuotaResponse = match  self.request(url).await{
            Ok(res)=>res.unwrap(),
            Err(err)=>{
                error!("get_useage_quota fail:{:?}",err);
                return Err(err);
            }
        }; 
        Ok((res.quota.usage, res.quota.limit))
    }

    async fn list_files_and_cache( &self, path_str: String, parent_file_id: String)-> Result<Vec<WebdavFile>>{
        let mut pagetoken = "".to_string();
        let mut files = Vec::new();
        let access_token_key = "access_token".to_string();
        let access_token = self.auth_cache.get(&access_token_key).unwrap();

        loop{
            let mut rurl = format!("https://api-drive.mypikpak.com/drive/v1/files?parent_id={}&thumbnail_size=SIZE_LARGE&with_audit=true&page_token={}&limit=0&filters={{\"phase\":{{\"eq\":\"PHASE_TYPE_COMPLETE\"}},\"trashed\":{{\"eq\":false}}}}",&parent_file_id,pagetoken);
            if self.proxy_url.len()>4{
                rurl = format!("{}/https://api-drive.mypikpak.com/drive/v1/files?parent_id={}&thumbnail_size=SIZE_LARGE&with_audit=true&page_token={}&limit=0&filters={{\"phase\":{{\"eq\":\"PHASE_TYPE_COMPLETE\"}},\"trashed\":{{\"eq\":false}}}}",&self.proxy_url,&parent_file_id,pagetoken);
            }
            let url = rurl;

            let v: FilesList = self.request(url)
            .await?
            .context("expect response")?;
            if(v.next_page_token.is_empty()||v.next_page_token==""){
                let mut tempfiles =v.files.clone();
                files.append(&mut tempfiles);
                break;
            }else{
                let mut tempfiles =v.files.clone();
                files.append(&mut tempfiles);
                pagetoken = v.next_page_token;
            }
        }

        self.cache_dir(path_str,files.clone()).await;
        Ok(files)

    }

    async fn cache_dir(&self, dir_path: String, files: Vec<WebdavFile>) {
        trace!(path = %dir_path, count = files.len(), "cache dir");
        self.dir_cache.insert(dir_path, files).await;
    }

    fn find_in_cache(&self, path: &Path) -> Result<Option<WebdavFile>, FsError> {
        if let Some(parent) = path.parent() {
            let parent_str = parent.to_string_lossy().into_owned();
            let file_name = path
                .file_name()
                .ok_or(FsError::NotFound)?
                .to_string_lossy()
                .into_owned();
            let file = self.dir_cache.get(&parent_str).and_then(|files| {
                for file in &files {
                    if file.name == file_name {
                        return Some(file.clone());
                    }
                }
                None
            });
            Ok(file)
        } else {
            let root = WebdavFile::new_root();
            Ok(Some(root))
        }
    }
  
    async fn read_dir_and_cache(&self, path: PathBuf) -> Result<Vec<WebdavFile>, FsError> {
        let path_str = path.to_string_lossy().into_owned();
        debug!(path = %path_str, "read_dir and cache");
        let parent_file_id = if path_str == "/" {
            "".to_string()
        } else {
            match self.find_in_cache(&path) {
                Ok(Some(file)) => file.id,
                _ => {
                    if let Ok(Some(file)) = self.get_by_path(&path_str).await {
                        file.id
                    } else {
                        return Err(FsError::NotFound);
                    }
                }
            }
        };
        let mut files = if let Some(files) = self.dir_cache.get(&path_str) {
            files
        } else {
            self.list_files_and_cache(path_str, parent_file_id.clone()).await.map_err(|_| FsError::NotFound)?
        };

        let uploading_files = self.list_uploading_files(&parent_file_id);
        if !uploading_files.is_empty() {
            debug!("added {} uploading files", uploading_files.len());
            files.extend(uploading_files);
        }

        Ok(files)
    }


    fn list_uploading_files(&self, parent_file_id: &str) -> Vec<WebdavFile> {
        self.uploading
            .get(parent_file_id)
            .map(|val_ref| val_ref.value().clone())
            .unwrap_or_default()
    }


    fn remove_uploading_file(&self, parent_file_id: &str, name: &str) {
        if let Some(mut files) = self.uploading.get_mut(parent_file_id) {
            if let Some(index) = files.iter().position(|x| x.name == name) {
                files.swap_remove(index);
            }
        }
    }

    pub async fn get_by_path(&self, path: &str) -> Result<Option<WebdavFile>> {
        debug!(path = %path, "get file by path");
        if path == "/" || path.is_empty() {
            return Ok(Some(WebdavFile::new_root()));
        }
        let tpath = PathBuf::from(path);
        let path_str = tpath.to_string_lossy().into_owned();
        let file = self.find_in_cache(&tpath)?;
        if let Some(file) = file {
            Ok(Some(file))
        } else {
            let parts: Vec<&str> = path_str.split('/').collect();
            let parts_len = parts.len();
            let filename = parts[parts_len - 1];
            let mut prefix = PathBuf::from("/");
            for part in &parts[0..parts_len - 1] {
                let parent = prefix.join(part);
                prefix = parent.clone();
                let files = self.dir_cache.get(&parent.to_string_lossy().into_owned()).unwrap();
                if let Some(file) = files.iter().find(|f| &f.name == filename) {
                    return Ok(Some(file.clone()));
                }
            }
            Ok(Some(WebdavFile::new_root()))
        }
    
    }


    async fn get_file(&self, path: PathBuf) -> Result<Option<WebdavFile>, FsError> {

        let path_str = path.to_string_lossy().into_owned();
        debug!(path = %path_str, "get_file");

        // let pos = path_str.rfind('/').unwrap();
        // let path_length = path_str.len()-pos;
        // let path_name: String = path_str.chars().skip(pos+1).take(path_length).collect();

        let parts: Vec<&str> = path_str.split('/').collect();
        let parts_len = parts.len();
        let path_name = parts[parts_len - 1];

        // 忽略 macOS 上的一些特殊文件
        if path_name == ".DS_Store" || path_name.starts_with("._") {
            return Err(FsError::NotFound);
        }

        let file = self.find_in_cache(&path)?;
        if let Some(file) = file {
            trace!(path = %path.display(), file_id = %file.id, "file found in cache");
            Ok(Some(file))
        } else {

            debug!(path = %path.display(), "file not found in cache");

            // trace!(path = %path.display(), "file not found in cache");
            // if let Ok(Some(file)) = self.get_by_path(&path_str).await {
            //     return Ok(Some(file));
            // }
            let parts: Vec<&str> = path_str.split('/').collect();
            let parts_len = parts.len();
            let filename = parts[parts_len - 1];
            let mut prefix = PathBuf::from("/");
            for part in &parts[0..parts_len - 1] {
                let parent = prefix.join(part);
                prefix = parent.clone();
                let files = self.read_dir_and_cache(parent).await?;
                if let Some(file) = files.iter().find(|f| f.name == filename) {
                    trace!(path = %path.display(), file_id = %file.id, "file found in cache");
                    return Ok(Some(file.clone()));
                }
            }
            Ok(None)
        }

    }

    async fn get_download_url(&self,file_id: &str) -> Result<String> {
        debug!("get_download_url");
        let mut rurl = format!("https://api-drive.mypikpak.com/drive/v1/files/{}",file_id.to_string());
        if self.proxy_url.len()>4{
            rurl = format!("{}/https://api-drive.mypikpak.com/drive/v1/files/{}",&self.proxy_url,file_id.to_string());
        }

        let url = rurl;
        let res: WebdavFile = self.request(url)
            .await?
            .context("expect response")?;
        if(res.mime_type.contains("video/")){
            Ok(res.medias[0].link.url.clone())
        }else{
            Ok(res.web_content_link.clone())
        }

    }


    pub async fn download(&self, url: &str, start_pos: u64, size: usize) -> Result<Bytes> {
        let end_pos = start_pos + size as u64 - 1;
        debug!(url = %url, start = start_pos, end = end_pos, "download file");
        let range = format!("bytes={}-{}", start_pos, end_pos);
        let res = self.client
            .get(url)
            .header(RANGE, range)
            .timeout(Duration::from_secs(120))
            .send()
            .await?
            .error_for_status()?;
        Ok(res.bytes().await?)

    }


    pub async fn create_file_with_proof(&self,name: &str, parent_file_id: &str, hash:&str, size: u64,chunk_count: u64) ->  Result<UploadResponse> {
        let mut url = format!("https://api-drive.mypikpak.com/drive/v1/files");
        if self.proxy_url.len()>4{
            url = format!("{}/https://api-drive.mypikpak.com/drive/v1/files",&self.proxy_url);
        }
        let req = UploadRequest{
            kind:"drive#file".to_string(),
		    name:name.to_string(),
		    size:size,
		    hash: hash.to_string(),
		    upload_type: "UPLOAD_TYPE_RESUMABLE".to_string(),
            objProvider: ObjProvider { provider: "UPLOAD_TYPE_UNKNOWN".to_string() },
		    parent_id:parent_file_id.to_string(),
        };
        let payload = serde_json::to_string(&req).unwrap();
        let access_token_key = "access_token".to_string();
        let access_token = self.auth_cache.get(&access_token_key).unwrap();

        let res = self.client.post(url)
            .header(reqwest::header::CONTENT_LENGTH, payload.len())
            .header(reqwest::header::HOST, "api-drive.mypikpak.com")
            .header(reqwest::header::AUTHORIZATION, format!("Bearer {}",access_token))
            .body(payload)
            .send()
            .await?;

        let body = &res.text().await?;
        let result = match serde_json::from_str::<UploadResponse>(body) {
            Ok(result) => result,
            Err(e) => {
                error!(error = %e, "create_file_with_proof");
                return Err(e.into());
            }
        };
    
        Ok(result)
    }


    pub async fn get_pre_upload_info(&self,oss_args:&OssArgs) -> Result<String> {
        let mut url = format!("https://{}/{}?uploads",oss_args.endpoint,oss_args.key);
        if self.proxy_url.len()>4{
            url = format!("{}/https://{}/{}?uploads",&self.proxy_url,oss_args.endpoint,oss_args.key);
        }
        let now = SystemTime::now();
        let gmt = httpdate::fmt_http_date(now);
        let mut req = self.client.post(url)
            .header(reqwest::header::USER_AGENT, "aliyun-sdk-android/2.9.5(Linux/Android 11/ONEPLUS%20A6000;RKQ1.201217.002)")
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .header("X-Oss-Security-Token", &oss_args.security_token)
            .header("Date", &gmt).build()?;
        let oss_sign:String = self.hmac_authorization(&req,&gmt,oss_args);
        let oss_header = format!("OSS {}:{}",&oss_args.access_key_id,&oss_sign);
        let header_auth = HeaderValue::from_str(&oss_header).unwrap();
        req.headers_mut().insert(reqwest::header::AUTHORIZATION, header_auth);
        let res = self.client.execute(req).await?;
        let body = &res.text().await?;

        let result: InitiateMultipartUploadResult = from_str(body).unwrap();
        Ok(result.UploadId.clone())
    }

    pub async fn upload_chunk(&self, file:&WebdavFile, oss_args:&OssArgs, upload_id:&str, current_chunk:u64,body: Bytes) -> Result<(PartInfo)> {
        debug!(file_name=%file.name,upload_id = upload_id,current_chunk=current_chunk, "upload_chunk");
        let encoded: String = form_urlencoded::Serializer::new(String::new())
        .append_pair("partNumber", current_chunk.to_string().as_str())
        .append_pair("uploadId", upload_id)
        .finish();

        let mut url = format!("https://{}/{}?{}",oss_args.endpoint,oss_args.key,encoded);
        if self.proxy_url.len()>4{
            url = format!("{}/https://{}/{}?{}",&self.proxy_url,oss_args.endpoint,oss_args.key,encoded);
        }
  
        let now = SystemTime::now();
        let gmt = httpdate::fmt_http_date(now);
        let mut req = self.client.put(url)
            .body(body)
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .header("X-Oss-Security-Token", &oss_args.security_token)
            .header("Date", &gmt).build()?;
        let oss_sign:String = self.hmac_authorization(&req,&gmt,oss_args);
        let oss_header = format!("OSS {}:{}",&oss_args.access_key_id,&oss_sign);
        let header_auth = HeaderValue::from_str(&oss_header).unwrap();
        req.headers_mut().insert(reqwest::header::AUTHORIZATION, header_auth);
        let res = self.client.execute(req).await?;
        //let body = &res.text().await?;

        let etag  = match &res.headers().get("ETag") {
            Some(etag) => etag.to_str().unwrap().to_string(),
            None => "".to_string(),
        };

        let part = PartInfo {
            PartNumber: PartNumber { PartNumber: current_chunk },
            ETag: etag,
        };
        
        Ok(part)
    }


    pub async fn complete_upload(&self,file:&WebdavFile, upload_tags:String, oss_args:&OssArgs, upload_id:&str)-> Result<()> {
        info!(file = %file.name, "complete_upload");
        let url = format!("https://{}/{}?uploadId={}",oss_args.endpoint,oss_args.key,upload_id);
        let now = SystemTime::now();
        let gmt = httpdate::fmt_http_date(now);
        let mut req = self.client.post(url)
            .body(upload_tags)
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .header("X-Oss-Security-Token", &oss_args.security_token)
            .header("Date", &gmt).build()?;
        let oss_sign:String = self.hmac_authorization(&req,&gmt,oss_args);
        let oss_header = format!("OSS {}:{}",&oss_args.access_key_id,&oss_sign);
        let header_auth = HeaderValue::from_str(&oss_header).unwrap();
        req.headers_mut().insert(reqwest::header::AUTHORIZATION, header_auth);
        let res = self.client.execute(req).await?;

        Ok(())
    }




    pub fn hmac_authorization(&self, req:&reqwest::Request,time:&str,oss_args:&OssArgs)->String{
        // let headers = req.headers().clone();
        // let method_str = format!("{}",req.method().as_str());
        // let content_type_str = format!("{}",headers.get(reqwest::header::CONTENT_TYPE).unwrap().to_str().unwrap());
        // let mut string_builder = String::new();
        // string_builder.push_str(&method_str);
        // string_builder.push_str("\n");
        // string_builder.push_str(&content_type_str);
        // string_builder.push_str("\n");
        // string_builder.push_str(&time);
        // string_builder.push_str("\n");
        // let mut sorted_headers = headers.iter().collect::<Vec<_>>();
        // for header in req.headers().iter() {
        //     let header_name = header.0.as_str();
        //     if  header_name.contains("x-oss-"){
        //         sorted_headers.push(header);
        //     }
        // }
        // for sh in sorted_headers.iter() {
        //     let header_str = format!("{}:{}",sh.0.as_str(),sh.1.to_str().unwrap());
        //     string_builder.push_str(&header_str);
        //     string_builder.push_str("\n");
        // }
        // let oss_query_string = format!("/{}{}?{}",oss_args.bucket,req.url().path(),req.url().query().unwrap());
        // string_builder.push_str(&oss_query_string);
        //let message = string_builder;

        let message = format!("{}\n\n{}\n{}\nx-oss-security-token:{}\n/{}{}?{}",req.method().as_str(),req.headers().get(reqwest::header::CONTENT_TYPE).unwrap().to_str().unwrap(),time,oss_args.security_token,oss_args.bucket,req.url().path(),req.url().query().unwrap());
        let key = &oss_args.access_key_secret;
      
        let mut hasher = HmacSha::from(key, &message, Sha1::default());
        let result = hasher.compute_digest();
        let signature_base64 = base64::encode(&result);
        signature_base64
    }
   

    fn normalize_dav_path(&self, dav_path: &DavPath) -> PathBuf {
        let path = dav_path.as_pathbuf();
        if self.root.parent().is_none() || path.starts_with(&self.root) {
            return path;
        }
        let rel_path = dav_path.as_rel_ospath();
        if rel_path == Path::new("") {
            return self.root.clone();
        }
        self.root.join(rel_path)
    }
}

impl DavFileSystem for WebdavDriveFileSystem {
    fn open<'a>(
        &'a self,
        dav_path: &'a DavPath,
        options: OpenOptions,
    ) -> FsFuture<Box<dyn DavFile>> {
        let path = self.normalize_dav_path(dav_path);
        let mode = if options.write { "write" } else { "read" };
        debug!(path = %path.display(), mode = %mode, "fs: open");
        async move {
            if options.append {
                // Can't support open in write-append mode
                error!(path = %path.display(), "unsupported write-append mode");
                return Err(FsError::NotImplemented);
            }
            let parent_path = path.parent().ok_or(FsError::NotFound)?;
            let parent_file = self
                .get_file(parent_path.to_path_buf())
                .await?
                .ok_or(FsError::NotFound)?;
            let sha1 = options.checksum.and_then(|c| {
                if let Some((algo, hash)) = c.split_once(':') {
                    if algo.eq_ignore_ascii_case("sha1") {
                        Some(hash.to_string())
                    } else {
                        None
                    }
                } else {
                    None
                }
            });


            let dav_file = if let Some(mut file) = self.get_file(path.clone()).await? {
                if options.write && options.create_new {
                    return Err(FsError::Exists);
                }
                AliyunDavFile::new(self.clone(), file, parent_file.id,parent_path.to_path_buf(),options.size.unwrap_or_default(),sha1)
            } else if options.write && (options.create || options.create_new) {


                let size = options.size;
                let name = dav_path
                    .file_name()
                    .ok_or(FsError::GeneralFailure)?
                    .to_string();

                // 忽略 macOS 上的一些特殊文件
                if name == ".DS_Store" || name.starts_with("._") {
                    return Err(FsError::NotFound);
                }
                let now = SystemTime::now();

                let file_path = dav_path.as_url_string();
                let mut hasher = Sha1::default();
                hasher.update(file_path.as_bytes());
                let hash_code = hasher.finalize();
                let file_hash = format!("{:X}",&hash_code);
                let parent_folder_id = parent_file.id.clone();

                let file = WebdavFile {
                    name,
                    kind: "drive#file".to_string(),
                    id: "".to_string(),
                    parent_id: parent_folder_id,
                    phase:"".to_string(),
                    size: size.unwrap_or(0).to_string(),
                    created_time: DateTime::new(now),
                    modified_time: DateTime::new(now),
                    file_extension: "".to_string(),
                    mime_type: "".to_string(),
                    web_content_link: "".to_string(),
                    medias:Vec::new(),
                    hash:Some(file_hash),
                };
                let mut uploading = self.uploading.entry(parent_file.id.clone()).or_default();
                uploading.push(file.clone());

                AliyunDavFile::new(self.clone(), file, parent_file.id,parent_path.to_path_buf(),size.unwrap_or(0),sha1)
            } else {
                println!("FsError::NotFound");
                return Err(FsError::NotFound);
            };
            Ok(Box::new(dav_file) as Box<dyn DavFile>)
        }
        .boxed()
    }

    fn read_dir<'a>(
        &'a  self,
        path: &'a DavPath,
        _meta: ReadDirMeta,
    ) -> FsFuture<FsStream<Box<dyn DavDirEntry>>> {
        let path = self.normalize_dav_path(path);
        debug!(path = %path.display(), "fs: read_dir");
        async move {
            let files = self.read_dir_and_cache(path.clone()).await?;
            let mut v: Vec<Box<dyn DavDirEntry>> = Vec::with_capacity(files.len());
            for file in files {
                v.push(Box::new(file));
            }
            let stream = futures_util::stream::iter(v);
            Ok(Box::pin(stream) as FsStream<Box<dyn DavDirEntry>>)
        }
        .boxed()
    }

   


    fn create_dir<'a>(&'a self, dav_path: &'a DavPath) -> FsFuture<()> {
        let path = self.normalize_dav_path(dav_path);
        async move {
            let parent_path = path.parent().ok_or(FsError::NotFound)?;
            let parent_file = self
                .get_file(parent_path.to_path_buf())
                .await?
                .ok_or(FsError::NotFound)?;
            
            if !(parent_file.kind==String::from("drive#folder")) {
                return Err(FsError::Forbidden);
            }
            if let Some(name) = path.file_name() {
                let name = name.to_string_lossy().into_owned();
                self.create_folder(&parent_file.id,&name).await;
                self.dir_cache.invalidate(parent_path).await;
                Ok(())
            } else {
                Err(FsError::Forbidden)
            }
        }
        .boxed()
    }


    fn remove_dir<'a>(&'a self, dav_path: &'a DavPath) -> FsFuture<()> {
        let path = self.normalize_dav_path(dav_path);
        debug!(path = %path.display(), "fs: remove_dir");
        async move {

            let file = self
                .get_file(path.clone())
                .await?
                .ok_or(FsError::NotFound)?;

            if !(file.kind==String::from("drive#folder")) {
                return Err(FsError::Forbidden);
            }

            self.remove_file(&file.id)
                .await
                .map_err(|err| {
                    error!(path = %path.display(), error = %err, "remove directory failed");
                    FsError::GeneralFailure
                })?;
            self.dir_cache.invalidate(&path).await;
            self.dir_cache.invalidate_parent(&path).await;
            Ok(())
        }
        .boxed()
    }


    fn remove_file<'a>(&'a self, dav_path: &'a DavPath) -> FsFuture<()> {
        let path = self.normalize_dav_path(dav_path);
        debug!(path = %path.display(), "fs: remove_file");
        async move {
            let file = self
                .get_file(path.clone())
                .await?
                .ok_or(FsError::NotFound)?;

            self.remove_file(&file.id)
                .await
                .map_err(|err| {
                    error!(path = %path.display(), error = %err, "remove file failed");
                    FsError::GeneralFailure
                })?;
            self.dir_cache.invalidate_parent(&path).await;
            Ok(())
        }
        .boxed()
    }


    fn rename<'a>(&'a self, from_dav: &'a DavPath, to_dav: &'a DavPath) -> FsFuture<()> {
        let from = self.normalize_dav_path(from_dav);
        let to = self.normalize_dav_path(to_dav);
        debug!(from = %from.display(), to = %to.display(), "fs: rename");
        async move {
            let is_dir;
            if from.parent() == to.parent() {
                // rename
                if let Some(name) = to.file_name() {
                    let file = self
                        .get_file(from.clone())
                        .await?
                        .ok_or(FsError::NotFound)?;
                    is_dir = if file.kind == "drive#folder" {
                            true
                        } else {
                            false
                        };
                    let name = name.to_string_lossy().into_owned();
                    self.rename_file(&file.id, &name).await;
                } else {
                    return Err(FsError::Forbidden);
                }
            } else {
                // move
                let file = self
                    .get_file(from.clone())
                    .await?
                    .ok_or(FsError::NotFound)?;
                is_dir = if file.kind == "drive#folder" {
                    true
                } else {
                    false
                };
                let to_parent_file = self
                    .get_file(to.parent().unwrap().to_path_buf())
                    .await?
                    .ok_or(FsError::NotFound)?;
                let new_name = to_dav.file_name();
                self.move_file(&file.id, &to_parent_file.id).await;
            }


            if is_dir {
                self.dir_cache.invalidate(&from).await;
            }
            self.dir_cache.invalidate_parent(&from).await;
            self.dir_cache.invalidate_parent(&to).await;
            Ok(())
        }
        .boxed()
    }


    fn copy<'a>(&'a self, from_dav: &'a DavPath, to_dav: &'a DavPath) -> FsFuture<()> {
        let from = self.normalize_dav_path(from_dav);
        let to = self.normalize_dav_path(to_dav);
        debug!(from = %from.display(), to = %to.display(), "fs: copy");
        async move {
            let file = self
                .get_file(from.clone())
                .await?
                .ok_or(FsError::NotFound)?;
            let to_parent_file = self
                .get_file(to.parent().unwrap().to_path_buf())
                .await?
                .ok_or(FsError::NotFound)?;
            let new_name = to_dav.file_name();
            self.copy_file(&file.id, &to_parent_file.id).await;
            self.dir_cache.invalidate(&to).await;
            self.dir_cache.invalidate_parent(&to).await;
            Ok(())
        }
        .boxed()
    }



    fn get_quota(&self) -> FsFuture<(u64, Option<u64>)> {
        async move {
            let (used, total) = self.get_useage_quota().await.map_err(|err| {
                error!(error = %err, "get quota failed");
                FsError::GeneralFailure
            })?;
            Ok((used, Some(total)))
        }
        .boxed()
    }

    fn metadata<'a>(&'a self, path: &'a DavPath) -> FsFuture<Box<dyn DavMetaData>> {
        let path = self.normalize_dav_path(path);
        debug!(path = %path.display(), "fs: metadata");
        async move {
            let file = self.get_file(path).await?.ok_or(FsError::NotFound)?;
            Ok(Box::new(file) as Box<dyn DavMetaData>)
        }
        .boxed()
    }


    fn have_props<'a>(
        &'a self,
        _path: &'a DavPath,
    ) -> std::pin::Pin<Box<dyn futures_util::Future<Output = bool> + Send + 'a>> {
        Box::pin(ready(true))
    }

    fn get_prop(&self, dav_path: &DavPath, prop:DavProp) -> FsFuture<Vec<u8>> {
        let path = self.normalize_dav_path(dav_path);
        let prop_name = match prop.prefix.as_ref() {
            Some(prefix) => format!("{}:{}", prefix, prop.name),
            None => prop.name.to_string(),
        };
        debug!(path = %path.display(), prop = %prop_name, "fs: get_prop");
        async move {
            if prop.namespace.as_deref() == Some("http://owncloud.org/ns")
                && prop.name == "checksums"
            {
                let file = self.get_file(path).await?.ok_or(FsError::NotFound)?;
                if let Some(sha1) = file.hash {
                    let xml = format!(
                        r#"<?xml version="1.0"?>
                        <oc:checksums xmlns:d="DAV:" xmlns:nc="http://nextcloud.org/ns" xmlns:oc="http://owncloud.org/ns">
                            <oc:checksum>sha1:{}</oc:checksum>
                        </oc:checksums>
                    "#,
                        sha1
                    );
                    return Ok(xml.into_bytes());
                }
            }
            Err(FsError::NotImplemented)
        }
        .boxed()
    }





}

#[derive(Debug, Clone)]
struct UploadState {
    size: u64,
    buffer: BytesMut,
    chunk_count: u64,
    chunk: u64,
    upload_id: String,
    oss_args: Option<OssArgs>,
    sha1: Option<String>,
    upload_tags:CompleteMultipartUpload,
}

impl Default for UploadState {
    fn default() -> Self {
        let mut upload_tags = CompleteMultipartUpload{Part:vec![]};
        Self {
            size: 0,
            buffer: BytesMut::new(),
            chunk_count: 0,
            chunk: 1,
            upload_id: String::new(),
            oss_args: None,
            sha1: None,
            upload_tags: upload_tags,
        }
    }
}

#[derive(Clone)]
struct AliyunDavFile {
    fs: WebdavDriveFileSystem,
    file: WebdavFile,
    parent_file_id: String,
    parent_dir: PathBuf,
    current_pos: u64,
    download_url: Option<String>,
    upload_state: UploadState,
}

impl Debug for AliyunDavFile {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AliyunDavFile")
            .field("file", &self.file)
            .field("parent_file_id", &self.parent_file_id)
            .field("current_pos", &self.current_pos)
            .field("upload_state", &self.upload_state)
            .finish()
    }
}

impl AliyunDavFile {
    fn new(fs: WebdavDriveFileSystem, file: WebdavFile, parent_file_id: String,parent_dir: PathBuf,size: u64,sha1: Option<String>,) -> Self {
        Self {
            fs,
            file,
            parent_file_id,
            parent_dir,
            current_pos: 0,
            upload_state: UploadState {
                size,
                sha1,
                ..Default::default()
            },
            download_url: None,
        }
    }

    async fn get_download_url(&self) -> Result<String, FsError> {
        self.fs.get_download_url(&self.file.id).await.map_err(|err| {
            error!(file_id = %self.file.id, file_name = %self.file.name, error = %err, "get download url failed");
            FsError::GeneralFailure
        })
    }

    async fn prepare_for_upload(&mut self) -> Result<bool, FsError> {
        if self.upload_state.chunk_count == 0 {
            let size = self.upload_state.size;
            debug!(file_name = %self.file.name, size = size, "prepare for upload");

            if !self.file.id.is_empty() {
                if let Some(content_hash) = self.file.hash.as_ref() {
                    if let Some(sha1) = self.upload_state.sha1.as_ref() {
                        if content_hash.eq_ignore_ascii_case(sha1) {
                            debug!(file_name = %self.file.name, sha1 = %sha1, "skip uploading same content hash file");
                            return Ok(false);
                        }
                    }
                }

                if self.fs.skip_upload_same_size && self.file.size.parse::<u64>().unwrap() == size {
                    debug!(file_name = %self.file.name, size = size, "skip uploading same size file");
                    return Ok(false);
                }
                // existing file, delete before upload
                if let Err(err) = self
                    .fs
                    .remove_file(&self.file.id)
                    .await
                {
                    error!(file_name = %self.file.name, error = %err, "delete file before upload failed");
                }
            }
            // TODO: create parent folders?

            let upload_buffer_size = self.fs.upload_buffer_size as u64;
            let chunk_count =
                size / upload_buffer_size + if size % upload_buffer_size != 0 { 1 } else { 0 };
            self.upload_state.chunk_count = chunk_count;
            debug!("uploading {} ({} bytes)...", self.file.name, size);
            if size>0 {
                let hash = &self.file.clone().hash.unwrap();
                let res = self
                    .fs
                    .create_file_with_proof(&self.file.name, &self.parent_file_id, hash, size, chunk_count)
                    .await;
            
                let upload_response = match res {
                    Ok(upload_response_info) => upload_response_info,
                    Err(err) => {
                        error!(file_name = %self.file.name, error = %err, "create file with proof failed");
                        return Ok(false);
                    }
                };

                let oss_args = OssArgs {
                    bucket: upload_response.resumable.params.bucket.to_string(),
                    key: upload_response.resumable.params.key.to_string(),
                    endpoint: upload_response.resumable.params.endpoint.to_string(),
                    access_key_id: upload_response.resumable.params.access_key_id.to_string(),
                    access_key_secret: upload_response.resumable.params.access_key_secret.to_string(),
                    security_token: upload_response.resumable.params.security_token.to_string(),
                };
                self.upload_state.oss_args = Some(oss_args);
    
                let oss_args = self.upload_state.oss_args.as_ref().unwrap();
                let pre_upload_info = self.fs.get_pre_upload_info(&oss_args).await;
                if let Err(err) = pre_upload_info {
                    error!(file_name = %self.file.name, error = %err, "get pre upload info failed");
                    return Ok(false);
                }
               
                self.upload_state.upload_id = match pre_upload_info {
                    Ok(upload_id) => upload_id,
                    Err(err) => {
                        error!(file_name = %self.file.name, error = %err, "get pre upload info failed");
                        return Ok(false);
                    }
                };
                debug!(file_name = %self.file.name, upload_id = %self.upload_state.upload_id, "pre upload info get upload_id success");
            }
        }
        Ok(true)
    }

    async fn maybe_upload_chunk(&mut self, remaining: bool) -> Result<(), FsError> {
        let chunk_size = if remaining {
            // last chunk size maybe less than upload_buffer_size
            self.upload_state.buffer.remaining()
        } else {
            self.fs.upload_buffer_size
        };
        let current_chunk = self.upload_state.chunk;

        if chunk_size > 0
            && self.upload_state.buffer.remaining() >= chunk_size
            && current_chunk <= self.upload_state.chunk_count
        {
            let chunk_data = self.upload_state.buffer.split_to(chunk_size);
            debug!(
                file_id = %self.file.id,
                file_name = %self.file.name,
                size = self.upload_state.size,
                "upload part {}/{}",
                current_chunk,
                self.upload_state.chunk_count
            );
            let upload_data = chunk_data.freeze();
            let oss_args = match self.upload_state.oss_args.as_ref() {
                Some(oss_args) => oss_args,
                None => {
                    error!(file_name = %self.file.name, "获取文件上传信息错误");
                    return Err(FsError::GeneralFailure);
                }
            };
            let res = self.fs.upload_chunk(&self.file,oss_args,&self.upload_state.upload_id,current_chunk,upload_data.clone()).await;
            
            let part = match res {
                Ok(part) => part,
                Err(err) => {
                    error!(file_name = %self.file.name, error = %err, "上传分片失败，无法获取ETag");
                    return Err(FsError::GeneralFailure);
                }
            };
                




            debug!(chunk_count = %self.upload_state.chunk_count, current_chunk=current_chunk, "upload chunk info");
            self.upload_state.upload_tags.Part.push(part);

             
            if current_chunk == self.upload_state.chunk_count{
                debug!(file_name = %self.file.name, "upload finished");

                let mut buffer = Vec::new();
                let mut ser = XmlSerializer::with_root(Writer::new_with_indent(&mut buffer, b' ', 4), Some("CompleteMultipartUpload"));
                self.upload_state.upload_tags.serialize(&mut ser).unwrap();
                let upload_tags = String::from_utf8(buffer).unwrap();
                self.fs.complete_upload(&self.file,upload_tags,oss_args,&self.upload_state.upload_id).await;
                self.upload_state = UploadState::default();
                // self.upload_state.buffer.clear();
                // self.upload_state.chunk = 0;
                self.fs.dir_cache.invalidate(&self.parent_dir).await;
                info!("parent dir is  {} parent_file_id is {}", self.parent_dir.to_string_lossy().to_string(), &self.parent_file_id.to_string());
                self.fs.list_files_and_cache(self.parent_dir.to_string_lossy().to_string(), self.parent_file_id.to_string());
            }


            self.upload_state.chunk += 1;
        }


        

        Ok(())
    }

}

impl DavFile for AliyunDavFile {
    fn metadata(&'_ mut self) -> FsFuture<'_, Box<dyn DavMetaData>> {
        debug!(file_id = %self.file.id, file_name = %self.file.name, "file: metadata");
        async move {
            let file = self.file.clone();
            Ok(Box::new(file) as Box<dyn DavMetaData>)
        }
        .boxed()
    }

    fn write_buf(&'_ mut self, buf: Box<dyn Buf + Send>) -> FsFuture<'_, ()> {
        debug!(file_id = %self.file.id, file_name = %self.file.name, "file: write_buf");
        async move {
            if self.prepare_for_upload().await? {
                self.upload_state.buffer.put(buf);
                self.maybe_upload_chunk(false).await?;
            }
            Ok(())
        }
        .boxed()
    }

    fn write_bytes(&mut self, buf: Bytes) -> FsFuture<()> {
        debug!(file_id = %self.file.id, file_name = %self.file.name, "file: write_bytes");
        async move {
            if self.prepare_for_upload().await? {
                self.upload_state.buffer.extend_from_slice(&buf);
                self.maybe_upload_chunk(false).await?;
            }
            Ok(())
        }
        .boxed()
    }

    fn flush(&mut self) -> FsFuture<()> {
        debug!(file_id = %self.file.id, file_name = %self.file.name, "file: flush");
        async move {
            if self.prepare_for_upload().await? {
                self.maybe_upload_chunk(true).await?;
                self.fs.remove_uploading_file(&self.parent_file_id, &self.file.name);
                self.fs.dir_cache.invalidate(&self.parent_dir).await;
            }
            Ok(())
        }
        .boxed()
    }

    fn read_bytes(&mut self, count: usize) -> FsFuture<Bytes> {
        debug!(
            file_id = %self.file.id,
            file_name = %self.file.name,
            pos = self.current_pos,
            count = count,
            "file: read_bytes",
        );
        async move {
            if self.file.id.is_empty() {
                // upload in progress
                return Err(FsError::NotFound);
            }
            let download_url = self.download_url.take();
            let download_url = if let Some(mut url) = download_url {
                if is_url_expired(&url) {
                    debug!(url = %url, "download url expired");
                    url = self.get_download_url().await?;
                }
                url
            } else {
                self.get_download_url().await?
            };

            let content = self
                .fs
                .download(&download_url, self.current_pos, count)
                .await
                .map_err(|err| {
                    error!(url = %download_url, error = %err, "download file failed");
                    FsError::NotFound
                })?;
            self.current_pos += content.len() as u64;
            self.download_url = Some(download_url);
            Ok(content)
        }
        .boxed()
    }

    fn seek(&mut self, pos: SeekFrom) -> FsFuture<u64> {
        debug!(
            file_id = %self.file.id,
            file_name = %self.file.name,
            pos = ?pos,
            "file: seek"
        );
        async move {
            let new_pos = match pos {
                SeekFrom::Start(pos) => pos,
                SeekFrom::End(pos) => (self.file.size.parse::<u64>().unwrap() as i64 - pos) as u64,
                SeekFrom::Current(size) => self.current_pos + size as u64,
            };
            self.current_pos = new_pos;
            Ok(new_pos)
        }
        .boxed()
    }

   
}

fn is_url_expired(url: &str) -> bool {
    if let Ok(oss_url) = ::url::Url::parse(url) {
        let expires = oss_url.query_pairs().find_map(|(k, v)| {
            if k == "x-oss-expires" {
                if let Ok(expires) = v.parse::<u64>() {
                    return Some(expires);
                }
            }
            None
        });
        if let Some(expires) = expires {
            let current_ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_secs();
            // 预留 1s
            return current_ts >= expires - 1;
        }
    }
    false
}




