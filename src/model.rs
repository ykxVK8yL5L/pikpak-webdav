use std::ops;
use ::time::{format_description::well_known::Rfc3339, OffsetDateTime};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::{bail, Context, Result};
use bytes::Bytes;
use futures_util::future::FutureExt;
use reqwest::{
    header::{HeaderMap, HeaderValue},
    StatusCode,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use tokio::{
    sync::{oneshot, RwLock},
    time,
};
use tracing::{debug, error, info, warn};
use webdav_handler::fs::{DavDirEntry, DavMetaData, FsFuture, FsResult};


#[derive(Debug, Clone)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}


#[derive(Debug, Clone, Deserialize)]
pub struct RefreshTokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
    pub token_type: String,
}


#[derive(Debug, Clone,Serialize)]
pub struct DateTime(SystemTime);

impl DateTime {
    pub fn new(st: SystemTime) -> Self {
        Self(st)
    }
}

impl<'a> Deserialize<'a> for DateTime {
    fn deserialize<D: Deserializer<'a>>(deserializer: D) -> Result<Self, D::Error> {
        let dt = OffsetDateTime::parse(<&str>::deserialize(deserializer)?, &Rfc3339)
            .map_err(serde::de::Error::custom)?;
        Ok(Self(dt.into()))
    }
}

impl ops::Deref for DateTime {
    type Target = SystemTime;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileType {
    Folder,
    File,
}

#[derive(Debug, Clone,Serialize, Deserialize)]
pub struct WebdavFile {
    pub kind: String,
    pub id: String,
    pub parent_id: String,
    pub name: String,
    pub size: String,
    pub file_extension: String,
    pub mime_type: String,
    pub web_content_link: String,
    pub created_time: DateTime,
    pub modified_time: DateTime,
    pub medias:Vec<Media>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Media {
    pub media_name: String,
    pub link:Link,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesList {
    pub kind : String,
    pub next_page_token : String,
    pub files: Vec<WebdavFile>,
}





impl DavMetaData for WebdavFile {
    fn len(&self) -> u64 {
        //self.size
        self.size.parse::<u64>().unwrap()
    }

    fn modified(&self) -> FsResult<SystemTime> {
        Ok(*self.modified_time)
    }

    fn is_dir(&self) -> bool {
        //matches!(self.kind, String::from("drive#folder") )
        self.kind.eq("drive#folder")
    }

    fn created(&self) -> FsResult<SystemTime> {
        Ok(*self.created_time)
    }
}

impl DavDirEntry for WebdavFile {
    fn name(&self) -> Vec<u8> {
        self.name.as_bytes().to_vec()
    }

    fn metadata(&self) -> FsFuture<Box<dyn DavMetaData>> {
        async move { Ok(Box::new(self.clone()) as Box<dyn DavMetaData>) }.boxed()
    }
}

impl WebdavFile {
    pub fn new_root() -> Self {
        let now = SystemTime::now();
        Self {
            kind: "drive#folder".to_string(),
            id: "".to_string(),
            parent_id: "".to_string(),
            name: "root".to_string(),
            size: "0".to_string(),
            created_time: DateTime(now),
            modified_time: DateTime(now),
            file_extension: "".to_string(),
            mime_type: "".to_string(),
            web_content_link: "".to_string(),
            medias:Vec::new(),
        }
    }
}
