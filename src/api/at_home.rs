use std::rc::Rc;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterFileList {
    pub hash: String,
    pub data: Rc<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfoResponse {
    pub base_url: String,
    pub chapter: ChapterFileList,
}

#[cfg(test)]
mod test {
    #[tokio::test]
    async fn can_get_server_info_response() -> Result<(), reqwest::Error> {
        // Tomo-chan wa onna no ko! chapter 953.5
        let chapter_id = "417d64e1-6c88-48f8-b507-ad43e9636888";
        let url = url::Url::parse(&format!("https://api.mangadex.org/at-home/server/{}", chapter_id,)).unwrap();
        let resp = reqwest::get(url.clone())
            .await?
            .json::<super::ServerInfoResponse>()
            .await?;
        Ok(())
    }
}
