use std::rc::Rc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api;
use api::chapter::ChapterData;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MangaRelationShip {}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MangaFeedResponse {
    pub result: String,
    pub response: String,
    pub limit: usize,
    pub offset: usize,
    pub total: usize,
    pub data: Vec<ChapterData>,
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
    pub relationships: Vec<MangaRelationShip>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MangaResponse {
    pub result: String,
    pub data: MangaData,
}

#[cfg(test)]
mod test {
    #[tokio::test]
    async fn can_get_manga_response() -> Result<(), reqwest::Error> {
        // Tomo-chan wa onna no ko!
        // Url: https://api.mangadex.org/manga/76ee7069-23b4-493c-bc44-34ccbf3051a8/feed?offset=2&limit=10&translatedLanguage[]=en&order[volume]=asc&order[chapter]=asc
        let title_id = "76ee7069-23b4-493c-bc44-34ccbf3051a8";
        let offset = 2;
        let lang_code = "en";
        let url = url::Url::parse(&format!(
            "https://api.mangadex.org/manga/{}/feed?offset={}&limit=10&translatedLanguage[]={}&order[volume]=asc&order[chapter]=asc",
            title_id,
            offset,
            lang_code
        )).unwrap();
        crate::client::CLIENT
            .clone()
            .get(url.clone())
            .send()
            .await?
            .json::<super::MangaFeedResponse>()
            .await?;
        Ok(())
    }
}
