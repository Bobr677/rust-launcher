pub(crate) mod client;
mod conflicts;
mod content_metadata;
mod dependencies;
mod installed;
mod jar_compat;
pub(crate) mod types;
mod updates;

pub use types::ModrinthDownloadTarget;
pub use content_metadata::resolve_profile_item_metadata;
pub use conflicts::scan_profile_mod_jars;
pub use updates::{
    apply_profile_content_updates, check_profile_content_updates,
    check_profile_incompatible_content,
};

use std::path::PathBuf;

use client::modrinth_http_client;
use dependencies::collect_modrinth_required_downloads;
use futures_util::stream::{self, StreamExt};
use installed::{index_content_dir_sha1, mark_skipped_already_installed};

use crate::app::paths::{game_root_dir, instance_dir};
use crate::services::game::download::{modrinth_content_subdir, save_modrinth_file};

const DOWNLOAD_CONCURRENCY: usize = 12;

fn modrinth_content_dir(
    category: &str,
    profile_id: Option<&str>,
) -> Result<PathBuf, String> {
    let root = if let Some(id) = profile_id {
        instance_dir(id)?
    } else {
        game_root_dir()?
    };
    Ok(root.join(modrinth_content_subdir(category)?))
}

#[tauri::command]
pub async fn resolve_modrinth_required_dependencies(
    version_id: String,
    game_version: String,
    loader: String,
) -> Result<Vec<ModrinthDownloadTarget>, String> {
    let client = modrinth_http_client();
    collect_modrinth_required_downloads(&client, &version_id, &game_version, &loader).await
}

#[tauri::command]
pub async fn download_modrinth_with_dependencies(
    category: String,
    version_id: String,
    game_version: String,
    loader: String,
    profile_id: Option<String>,
) -> Result<Vec<ModrinthDownloadTarget>, String> {
    let client = modrinth_http_client();
    let mut targets =
        collect_modrinth_required_downloads(&client, &version_id, &game_version, &loader).await?;

    let content_dir = modrinth_content_dir(&category, profile_id.as_deref())?;
    let installed_index = index_content_dir_sha1(&content_dir).await?;
    mark_skipped_already_installed(&mut targets, &installed_index);

    let pending: Vec<&ModrinthDownloadTarget> = targets.iter().filter(|t| !t.skipped).collect();
    if !pending.is_empty() {
        let results: Vec<Result<(), String>> = stream::iter(pending.into_iter())
            .map(|target| {
                let category = category.clone();
                let profile_id = profile_id.clone();
                let url = target.url.clone();
                let filename = target.filename.clone();
                let sha1 = target.sha1.clone();
                async move {
                    save_modrinth_file(
                        &category,
                        &url,
                        &filename,
                        profile_id.as_deref(),
                        sha1.as_deref(),
                    )
                    .await
                }
            })
            .buffer_unordered(DOWNLOAD_CONCURRENCY)
            .collect()
            .await;

        for result in results {
            result?;
        }
    }

    Ok(targets)
}
