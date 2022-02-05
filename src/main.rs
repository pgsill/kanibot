extern crate image;
use image::{imageops::FilterType, open, DynamicImage, RgbImage};
use std::collections::VecDeque;
use std::env;
use std::error::Error;
use std::sync::{Arc, RwLock};
use substring::Substring;
use teloxide::prelude::*;
use teloxide::types::{MediaKind, MessageEntityKind, MessageKind};
use teloxide::{
    net::Download,
    requests::{Request, Requester},
    types::File as TgFile,
    Bot,
};
use tokio::fs::File;
mod commands;
use crate::commands::CommandsJson;

const SIMILARITY_THRESHOLD: f64 = 0.95;
const MIN_SIMILARITY_THRESHOLD: f64 = 0.8;
const MAX_SIMILARITY_THRESHOLD: f64 = 0.99;

const MOSAIC_SIZE: u32 = 9;
const MIN_MOSAIC_SIZE: u32 = 2;
const MAX_MOSAIC_SIZE: u32 = 12;

const MAX_RECENT_LINKS: usize = 50;
const MAX_RECENT_IMAGES: usize = 50;

fn make_3x3_mosaic(rgb_image: RgbImage, name: &str) -> RgbImage {
    let img = DynamicImage::ImageRgb8(rgb_image);

    let new_image = img.resize_exact(MOSAIC_SIZE, MOSAIC_SIZE, FilterType::Gaussian);

    new_image.save(format!("mosaic{}", name)).unwrap();

    return new_image.to_rgb8();
}

fn compare_mosaics(mos1: &RgbImage, mos2: &RgbImage) -> f64 {
    let (size_x, size_y) = &mos1.dimensions();

    let similarity_percent: f64;
    let pixel_count = size_x * size_y;
    let mut similarity_accumulator: f64 = 0.0;

    for pos_x in 0..*size_x {
        for pos_y in 0..*size_y {
            let [mos1_r, mos1_g, mos1_b] = &mos1.get_pixel(pos_x, pos_y).0;
            let [mos2_r, mos2_g, mos2_b] = &mos2.get_pixel(pos_x, pos_y).0;

            let difference = ((*mos2_r as i32 - *mos1_r as i32).pow(2)
                + (*mos2_g as i32 - *mos1_g as i32).pow(2)
                + (*mos2_b as i32 - *mos1_b as i32).pow(2)) as f64;
            let mut current_similarity_percentage =
                difference.sqrt() / ((255u32.pow(2) + 255u32.pow(2) + 255u32.pow(2)) as f64).sqrt();

            current_similarity_percentage = 1.0 - current_similarity_percentage;

            similarity_accumulator = similarity_accumulator + current_similarity_percentage;
        }
    }

    similarity_percent = similarity_accumulator / pixel_count as f64;

    return similarity_percent;
}

async fn get_photos_from_message(
    message: &UpdateWithCx<AutoSend<Bot>, Message>,
) -> Result<Option<Vec<String>>, Box<dyn Error>> {
    let mut saved_images: Vec<String> = vec![];

    match message.update.photo() {
        Some(photos) => {
            let photo = photos.last().unwrap();
            let file_id = &photo.file_id;

            let TgFile { file_path, .. } = message.requester.get_file(file_id).send().await?;

            let mut file = File::create(&file_path).await?;

            message
                .requester
                .download_file(&file_path, &mut file)
                .await?;

            saved_images.push(file_path);

            return Ok(Some(saved_images));
        }
        None => Ok(None),
    }
}

fn get_similar_image_posted_recently(
    image: RgbImage,
    recents: &mut VecDeque<RgbImage>,
    name: &str,
) -> bool {
    let newmosaic = make_3x3_mosaic(image, name);

    for (idx, recent_image) in recents.iter().enumerate() {
        let similarity_amount = compare_mosaics(&newmosaic, recent_image);
        println!(
            "similarity between images was: {:?}/1.0; Comparing with image #{:?}",
            similarity_amount, idx
        );

        if similarity_amount > SIMILARITY_THRESHOLD {
            return true;
        }
    }

    recents.push_front(newmosaic);

    if recents.len() > MAX_RECENT_IMAGES {
        recents.pop_back();
    }

    return false;
}

fn get_links_posted_recently(
    message_content: &MessageKind,
    recents: &mut VecDeque<String>,
) -> bool {
    match message_content {
        MessageKind::Common(message_content) => {
            let media_kind = &message_content.media_kind;

            match media_kind {
                MediaKind::Text(media_kind) => {
                    let text = str::to_string(&media_kind.text);

                    for entity in media_kind.entities.iter() {
                        let kind = &entity.kind;

                        if let MessageEntityKind::Url = kind {
                            let start_index = entity.offset;
                            let end_index = entity.offset + entity.length;
                            let url = text.substring(start_index, end_index).to_string();

                            // check if exists
                            if recents.contains(&url) {
                                return true;
                            } else {
                                recents.push_front(url);

                                if recents.len() > MAX_RECENT_LINKS {
                                    recents.pop_back();
                                }
                            }
                        }
                    }
                }

                _ => {}
            }
        }
        _ => {}
    }

    return false;
}

fn command_handler(
    message: &UpdateWithCx<AutoSend<Bot>, Message>,
    command_strings: &CommandsJson,
) -> Option<String> {
    let message_text = match message.update.text() {
        Some(text) => String::from(text),
        _ => String::from(""),
    };

    println!("Received message: {:?}", message_text);

    if command_strings.help.contains(&message_text) {
        let increase_similarity_threshold_string =
            command_strings.increaseSimilarityThreshold.join(" or ");
        let decrease_similarity_threshold_string =
            command_strings.decreaseSimilarityThreshold.join(" or ");
        let increase_mosaic_size_string = command_strings.increaseMosaicSize.join(" or ");
        let decrease_mosaic_size_string = command_strings.decreaseMosaicSize.join(" or ");

        println!("Received HELP REQUEST: {:?}", message_text);

        return Some(format!(
            "I can respond to the following prompts: {} / {} / {} / {}",
            increase_similarity_threshold_string,
            decrease_similarity_threshold_string,
            increase_mosaic_size_string,
            decrease_mosaic_size_string
        ));
    }

    println!(
        "{:?}",
        command_strings
            .increaseSimilarityThreshold
            .contains(&message_text)
    );
    println!(
        "{:?}",
        command_strings
            .decreaseSimilarityThreshold
            .contains(&message_text)
    );
    println!(
        "{:?}",
        command_strings.increaseMosaicSize.contains(&message_text)
    );
    println!(
        "{:?}",
        command_strings.decreaseMosaicSize.contains(&message_text)
    );

    return None;
}

#[tokio::main]
async fn main() {
    color_backtrace::install();
    teloxide::enable_logging!();
    log::info!("Starting dices_bot...");

    let recent_links = Arc::new(RwLock::new(VecDeque::new()));
    let recent_images = Arc::new(RwLock::new(VecDeque::new()));
    let command_strings = Arc::new(RwLock::new(commands::get_commands_json()));

    let bot = Bot::from_env().auto_send();

    teloxide::repl(bot, {
        move |message| {
            // atomic references
            let command_strings = Arc::clone(&command_strings);
            let recent_images = Arc::clone(&recent_images);
            let recent_links = Arc::clone(&recent_links);

            // process photos and links
            async move {
                if let Some(response) = command_handler(&message, &command_strings.read().unwrap())
                {
                    message.answer(response).await?;
                    return respond(());
                }

                let photos_in_message = get_photos_from_message(&message).await.unwrap();
                for photo in photos_in_message.unwrap_or_default() {
                    let image_file = match open(&photo) {
                        Ok(dynimg) => dynimg.into_rgb8(),
                        Err(_) => {
                            continue;
                        }
                    };
                    let image_found = get_similar_image_posted_recently(
                        image_file,
                        &mut recent_images.write().unwrap(),
                        &photo,
                    );

                    if image_found {
                        message
                            .answer("A similar image has been posted recently.")
                            .await?;
                        return respond(());
                    }
                }

                // if no images are processed,
                // process message as text
                let has_duplicate_message = get_links_posted_recently(
                    &message.update.kind,
                    &mut recent_links.write().unwrap(),
                );

                if has_duplicate_message {
                    message
                        .answer("Someone already posted that link. 😳✋😂👉🚪")
                        .await?;
                    return respond(());
                }

                return match message.update.text() {
                    None => Ok(()),
                    _ => Ok(()),
                };
            }
        }
    })
    .await;

    /* let bot = Bot::from_env().auto_send();

    teloxide::repl(bot, |message| async move {
        let photos_in_message = get_photos_from_message(&message).await.unwrap();
        for photo in photos_in_message.unwrap() {
            let image_file = open(photo).unwrap().into_rgb8();
            let image_found = get_similar_image_posted_recently(image_file, static_ref);

            if image_found {
                message
                    .answer("A similar image has been posted recently.")
                    .await?;
                return respond(());
            }
        }

        return match message.update.text() {
            None => Ok(()),
            Some(message_value) => {
                message.answer(message_value).await?;
                return respond(());
            }
        };
    })
    .await; */
}
