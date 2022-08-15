use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration,SystemTime, UNIX_EPOCH};
use anyhow::{bail, Context, Result};
use bytes::{Buf, BufMut, Bytes, BytesMut};
use dashmap::DashMap;
use futures_util::future::FutureExt;
use tracing::{debug, error, trace,info};
use webdav_handler::{
    davpath::DavPath,
    fs::{
        DavDirEntry, DavFile, DavFileSystem, DavMetaData, FsError, FsFuture, FsStream, OpenOptions,
        ReadDirMeta,
    },
};
use moka::future::{Cache as AuthCache};
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
use serde::Serialize;
use serde_json::json;
use reqwest::header::RANGE;

pub use crate::model::{WebdavFile, DateTime, FileType,FilesList,Credentials,RefreshTokenResponse,CreateFolderRequest,DelFileRequest,RenameFileRequest,MoveFileRequest,MoveTo};


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
}

impl WebdavDriveFileSystem {
    pub async fn new(
        credentials:Credentials,
        root: String,
        cache_size: u64,
        cache_ttl: u64,
        proxy_url: String
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
                //info!(refresh_token = %res.access_token, "refresh token succeed");
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

        let res: WebdavFile = self.post_request(url, &req)
        .await?
        .context("expect response")?;
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
        self.post_request(url, &req)
        .await?
        .context("expect response")?;
        Ok(())
    }

    pub async fn rename_file(&self, file_id: &str, new_name: &str) -> Result<()> {
        //println!("rename file {} to {}", file_id, new_name);
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


    async fn list_files_and_cache( &self, path_str: String, parent_file_id: String)-> Result<Vec<WebdavFile>>{
        //println!("list_files_and_cache {}",path_str);
        let mut pagetoken = "".to_string();
        let mut files = Vec::new();
        let access_token_key = "access_token".to_string();
        let access_token = self.auth_cache.get(&access_token_key).unwrap();

        loop{
            // let url = format!("https://api-drive.mypikpak.com/drive/v1/files?parent_id={}&thumbnail_size=SIZE_LARGE&with_audit=true&page_token={}&limit=0",&parent_file_id,pagetoken);
            // let res = self.client
            //     .get(url)
            //     .bearer_auth(&access_token)
            //     .send()
            //     .await?
            //     .error_for_status()?;
            // let data = res.text().await?;
            // let v:FilesList = serde_json::from_str(&data).unwrap();
            //let v: FilesList = self.request(format!("https://api-drive.mypikpak.com/drive/v1/files?parent_id={}&thumbnail_size=SIZE_LARGE&with_audit=true&page_token={}&limit=0&filters={{\"phase\":{{\"eq\":\"PHASE_TYPE_COMPLETE\"}},\"trashed\":{{\"eq\":false}}}}",&parent_file_id,pagetoken))
            //let v: FilesList = self.request(format!("https://api-drive.mypikpak.com/drive/v1/files?parent_id={}&thumbnail_size=SIZE_LARGE&with_audit=true&page_token={}&limit=0",&parent_file_id,pagetoken))
            
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
        info!(path = %path_str, "read_dir and cache");
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
        Ok(files)
    }

    pub async fn get_by_path(&self, path: &str) -> Result<Option<WebdavFile>> {
        info!(path = %path, "get file by path");
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
        info!(path = %path_str, "get_file");

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

            info!(path = %path.display(), "file not found in cache");

            trace!(path = %path.display(), "file not found in cache");
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
        info!("get_download_url");

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
        info!(url = %url, start = start_pos, end = end_pos, "download file");
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
        info!("fs:open  open file");

        let path = self.normalize_dav_path(dav_path);
        let mode = if options.write { "write" } else { "read" };
        info!(path = %path.display(), mode = %mode, "fs: open");
        
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
            let dav_file = if let Some(mut file) = self.get_file(path.clone()).await? {
                if options.write && options.create_new {
                    return Err(FsError::Exists);
                }
                if let Some(size) = options.size {
                    // 上传中的文件刚开始 size 可能为 0，更新为正确的 size
                    if file.size.parse::<u64>().unwrap()== 0 {
                        file.size = size.to_string();
                    }
                }
                AliyunDavFile::new(self.clone(), file, parent_file.id)
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
                let file = WebdavFile {
                    name,
                    kind: "drive#folder".to_string(),
                    id: "".to_string(),
                    parent_id: "".to_string(),
                    size: "0".to_string(),
                    created_time: DateTime::new(now),
                    modified_time: DateTime::new(now),
                    file_extension: "".to_string(),
                    mime_type: "".to_string(),
                    web_content_link: "".to_string(),
                    medias:Vec::new(),
                };

                AliyunDavFile::new(self.clone(), file, parent_file.id)
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
        info!(path = %path.display(), "fs: read_dir");
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
        //println!("create_dir {}",path.display());
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
        //println!("remove_dir {}",path.display());
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
        //println!("remove_file {}",path.display());
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
        //println!("rename {} {}",from.display(),to.display());
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


    fn metadata<'a>(&'a self, path: &'a DavPath) -> FsFuture<Box<dyn DavMetaData>> {
        let path = self.normalize_dav_path(path);
        info!(path = %path.display(), "fs: metadata");
        async move {
            let file = self.get_file(path).await?.ok_or(FsError::NotFound)?;
            Ok(Box::new(file) as Box<dyn DavMetaData>)
        }
        .boxed()
    }





}

#[derive(Debug, Clone)]
struct UploadState {
    buffer: BytesMut,
    chunk_count: u64,
    chunk: u64,
    upload_id: String,
    upload_urls: Vec<String>,
}

impl Default for UploadState {
    fn default() -> Self {
        Self {
            buffer: BytesMut::new(),
            chunk_count: 0,
            chunk: 1,
            upload_id: String::new(),
            upload_urls: Vec::new(),
        }
    }
}

#[derive(Clone)]
struct AliyunDavFile {
    fs: WebdavDriveFileSystem,
    file: WebdavFile,
    parent_file_id: String,
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
    fn new(fs: WebdavDriveFileSystem, file: WebdavFile, parent_file_id: String) -> Self {
        Self {
            fs,
            file,
            parent_file_id,
            current_pos: 0,
            download_url: None,
            upload_state: UploadState::default(),
        }
    }

    async fn get_download_url(&self) -> Result<String, FsError> {
        self.fs.get_download_url(&self.file.id).await.map_err(|err| {
            error!(file_id = %self.file.id, file_name = %self.file.name, error = %err, "get download url failed");
            FsError::GeneralFailure
        })
    }

    async fn prepare_for_upload(&mut self) -> Result<(), FsError> {
        Ok(())
    }

    async fn maybe_upload_chunk(&mut self, remaining: bool) -> Result<(), FsError> {
        Ok(())
    }
}

impl DavFile for AliyunDavFile {
    fn metadata(&'_ mut self) -> FsFuture<'_, Box<dyn DavMetaData>> {
        info!(file_id = %self.file.id, file_name = %self.file.name, "file: metadata");
        async move {
            let file = self.file.clone();
            Ok(Box::new(file) as Box<dyn DavMetaData>)
        }
        .boxed()
    }

    fn write_buf(&'_ mut self, buf: Box<dyn Buf + Send>) -> FsFuture<'_, ()> {
        debug!(file_id = %self.file.id, file_name = %self.file.name, "file: write_buf");
        async move {
            Ok(())
        }
        .boxed()
    }

    fn write_bytes(&mut self, buf: Bytes) -> FsFuture<()> {
        debug!(file_id = %self.file.id, file_name = %self.file.name, "file: write_bytes");
        async move {
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

    fn flush(&mut self) -> FsFuture<()> {
        debug!(file_id = %self.file.id, file_name = %self.file.name, "file: flush");
        async move {
            Ok(())
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




