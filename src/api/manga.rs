use std::rc::Rc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api;
use api::chapter::ChapterResponse;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MangaRelationShip {}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MangaFeedResponse {
    pub limit: usize,
    pub offset: usize,
    pub total: usize,
    pub results: Vec<ChapterResponse>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MangaAttributes {
    pub title: Option<String>,
    pub chapter: Option<String>,
    pub hash: String,
    pub data: Rc<Vec<String>>,
    pub translated_language: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MangaData {
    pub id: Uuid,
    #[serde(rename = "type")]
    pub data_type: String,
    pub attributes: MangaAttributes,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MangaResponse {
    pub result: String,
    pub data: MangaData,
    pub relationships: Vec<MangaRelationShip>,
}
