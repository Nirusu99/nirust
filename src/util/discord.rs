use std::{fs::File, path::PathBuf};

use serenity::model::channel::Attachment;

pub async fn download_attachements(attachments: &Vec<Attachment>, dir: &PathBuf) -> Vec<File> {}
