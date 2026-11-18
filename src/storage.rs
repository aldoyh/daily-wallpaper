use anyhow::{Context, Result};
use directories::ProjectDirs;
use std::fs;
use std::path::PathBuf;

const APPLICATION_QUALIFIER: &str = "com";
const APPLICATION_ORGANIZATION: &str = "Manus";
const APPLICATION_NAME: &str = "Daily Wallpaper";

#[derive(Debug, Clone)]
pub struct AppPaths {
    data_dir: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Result<Self> {
        let project_dirs = ProjectDirs::from(
            APPLICATION_QUALIFIER,
            APPLICATION_ORGANIZATION,
            APPLICATION_NAME,
        )
        .context("could not determine the platform application-data directory")?;
        Ok(Self {
            data_dir: project_dirs.data_local_dir().to_path_buf(),
        })
    }

    pub fn current_image(&self) -> PathBuf {
        self.data_dir.join("current.jpg")
    }

    pub fn save_image(&self, bytes: &[u8]) -> Result<PathBuf> {
        if bytes.is_empty() {
            anyhow::bail!("refusing to save an empty image");
        }
        fs::create_dir_all(&self.data_dir).with_context(|| {
            format!(
                "creating application data directory {}",
                self.data_dir.display()
            )
        })?;
        let destination = self.current_image();
        let temporary = self
            .data_dir
            .join(format!(".current-{}.tmp", std::process::id()));
        fs::write(&temporary, bytes)
            .with_context(|| format!("writing temporary image {}", temporary.display()))?;
        #[cfg(target_os = "windows")]
        if destination.exists() {
            fs::remove_file(&destination)
                .with_context(|| format!("replacing cached image {}", destination.display()))?;
        }
        if let Err(error) = fs::rename(&temporary, &destination) {
            let _ = fs::remove_file(&temporary);
            return Err(error)
                .with_context(|| format!("installing cached image {}", destination.display()));
        }
        Ok(destination)
    }

    pub fn for_test(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AppPaths;
    use std::fs;

    #[test]
    fn saves_a_non_empty_image_to_the_application_cache() {
        let directory =
            std::env::temp_dir().join(format!("daily-wallpaper-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        let paths = AppPaths::for_test(&directory);
        let saved = paths.save_image(b"image-bytes").expect("image should save");
        assert_eq!(
            fs::read(saved).expect("saved image should be readable"),
            b"image-bytes"
        );
        let saved_again = paths
            .save_image(b"new-image")
            .expect("image should replace");
        assert_eq!(
            fs::read(saved_again).expect("replaced image should be readable"),
            b"new-image"
        );
        let _ = fs::remove_dir_all(directory);
    }
}
