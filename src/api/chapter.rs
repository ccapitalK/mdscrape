use serde::{Deserialize, Serialize};
use uuid::Uuid;

use std::rc::Rc;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterAttributes {
    pub title: Option<String>,
    pub chapter: Option<String>,
    pub hash: String,
    pub data: Rc<Vec<String>>,
    pub translated_language: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterData {
    pub id: Uuid,
    #[serde(rename = "type")]
    pub data_type: String,
    pub attributes: ChapterAttributes,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterRelationShip {}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterResponse {
    pub result: String,
    pub data: ChapterData,
    pub relationships: Vec<ChapterRelationShip>,
}
