use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterAttributes {
    pub title: Option<String>,
    pub chapter: Option<String>,
    pub pages: usize,
    pub translated_language: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterData {
    pub id: Uuid,
    #[serde(rename = "type")]
    pub data_type: String,
    pub attributes: ChapterAttributes,
    pub relationships: Vec<ChapterRelationShip>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterRelationShip {
    id: String,
    #[serde(rename = "type")]
    relationship_type: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterResponse {
    pub result: String,
    pub response: String,
    pub data: ChapterData,
}

#[cfg(test)]
mod test {
    #[tokio::test]
    async fn can_get_chapter_response() -> Result<(), reqwest::Error> {
        use crate::client::CLIENT;
        // Tomo-chan wa onna no ko! chapter 953.5
        // Url: https://api.mangadex.org/chapter/417d64e1-6c88-48f8-b507-ad43e9636888
        let chapter_id = "417d64e1-6c88-48f8-b507-ad43e9636888";
        let url = url::Url::parse(&format!("https://api.mangadex.org/chapter/{}", chapter_id,)).unwrap();
        CLIENT
            .clone()
            .get(url.clone())
            .send()
            .await?
            .json::<super::ChapterResponse>()
            .await?;
        Ok(())
    }
}
