use anyhow::{anyhow, Context, Result};
use reqwest::blocking::{Client, Response};
use serde::Deserialize;
use std::io::Read;
use std::time::Duration;

const MAX_IMAGE_BYTES: u64 = 25 * 1024 * 1024;
const MAX_METADATA_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Bing,
    Random,
}

impl Source {
    pub const ALL: [Self; 2] = [Self::Bing, Self::Random];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Bing => "Bing image of the day",
            Self::Random => "Random photo",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DownloadedImage {
    pub bytes: Vec<u8>,
    pub title: String,
    pub credit: String,
    pub source: Source,
}

#[derive(Debug, Deserialize)]
struct BingResponse {
    images: Vec<BingImage>,
}

#[derive(Debug, Deserialize)]
struct BingImage {
    url: String,
    #[serde(default)]
    copyright: String,
    #[serde(default)]
    title: String,
}

pub fn download(source: Source) -> Result<DownloadedImage> {
    let client = client()?;
    match source {
        Source::Bing => download_bing(&client),
        Source::Random => download_random(&client),
    }
}

fn download_bing(client: &Client) -> Result<DownloadedImage> {
    let api = "https://www.bing.com/HPImageArchive.aspx?format=js&idx=0&n=1&mkt=en-US";
    let response = client
        .get(api)
        .send()
        .context("requesting Bing image metadata")?
        .error_for_status()
        .context("Bing returned an error")?;
    let metadata = read_bounded(response, MAX_METADATA_BYTES).context("reading Bing metadata")?;
    let response: BingResponse =
        serde_json::from_slice(&metadata).context("parsing Bing image metadata")?;
    let image = response
        .images
        .first()
        .ok_or_else(|| anyhow!("Bing did not return an image"))?;
    let url = format!("https://www.bing.com{}", image.url);
    let bytes = download_bytes(client.get(url).send().context("downloading Bing image")?)?;
    Ok(DownloadedImage {
        bytes,
        title: if image.title.is_empty() {
            "Bing image of the day".into()
        } else {
            image.title.clone()
        },
        credit: if image.copyright.is_empty() {
            "Bing".into()
        } else {
            image.copyright.clone()
        },
        source: Source::Bing,
    })
}

fn download_random(client: &Client) -> Result<DownloadedImage> {
    let bytes = download_bytes(
        client
            .get("https://picsum.photos/1920/1080")
            .send()
            .context("downloading random photo")?,
    )?;
    Ok(DownloadedImage {
        bytes,
        title: "Random photo".into(),
        credit: "Picsum Photos".into(),
        source: Source::Random,
    })
}

fn read_bounded(response: Response, maximum: u64) -> Result<Vec<u8>> {
    let response = response
        .error_for_status()
        .context("image provider returned an error")?;
    if let Some(length) = response.content_length() {
        if length > maximum {
            anyhow::bail!("response is too large ({length} bytes; maximum is {maximum})");
        }
    }
    let mut bytes = Vec::new();
    response
        .take(maximum + 1)
        .read_to_end(&mut bytes)
        .context("reading image bytes")?;
    if bytes.len() as u64 > maximum {
        anyhow::bail!("response exceeded the {} MiB limit", maximum / 1024 / 1024);
    }
    if bytes.is_empty() {
        anyhow::bail!("provider returned an empty response");
    }
    Ok(bytes)
}

fn download_bytes(response: Response) -> Result<Vec<u8>> {
    read_bounded(response, MAX_IMAGE_BYTES)
}

fn client() -> Result<Client> {
    Client::builder()
        .user_agent("DailyWallpaper/0.2")
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(45))
        .build()
        .context("building HTTP client")
}

#[cfg(test)]
mod tests {
    use super::Source;

    #[test]
    fn source_labels_are_user_friendly() {
        assert_eq!(Source::Bing.label(), "Bing image of the day");
        assert_eq!(Source::Random.label(), "Random photo");
    }
}
