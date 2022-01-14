extern crate image;
use image::io::Reader as ImageReader;
use image::{open, DynamicImage, GenericImage, GenericImageView, ImageBuffer, RgbImage};
use std::error::Error;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::{env, process};
use teloxide::{
    net::Download,
    requests::{Request, Requester},
    types::File as TgFile,
    Bot,
};
use teloxide::{prelude::*, types::PhotoSize, RequestError};
use tokio::fs::File;

const SIMILARITY_THRESHOLD: f64 = 0.85;

fn get_belonging_third(position: u32, size: u32) -> u32 {
    if position <= size {
        return 0;
    } else if position > size && position < size * 2 {
        return 1;
    } else {
        return 2;
    }
}

fn make_3x3_mosaic(rgbImage: RgbImage, name: &str) -> RgbImage {
    let img = DynamicImage::ImageRgb8(rgbImage);

    let img_size = img.dimensions();
    let x_third_size = img_size.0 / 3;
    let y_third_size = img_size.1 / 3;

    let mut new_image: RgbImage = ImageBuffer::new(3, 3);

    for (x, y, rgba) in img.pixels() {
        let x_map_position = get_belonging_third(x, x_third_size);
        let y_map_position = get_belonging_third(y, y_third_size);

        let [curr_r, curr_g, curr_b, _] = rgba.0;
        let target_pixel = *new_image.get_pixel(x_map_position, y_map_position);
        let [target_r, target_g, target_b] = target_pixel.0;

        let resulting_r = (curr_r as u32 + target_r as u32) / 2;
        let resulting_g = (target_g as u32 + curr_g as u32) / 2;
        let resulting_b = (target_b as u32 + curr_b as u32) / 2;

        new_image.put_pixel(
            x_map_position,
            y_map_position,
            image::Rgb([resulting_r as u8, resulting_g as u8, resulting_b as u8]),
        )
    }

    new_image.save(format!("mosaic{}", name)).unwrap();

    return new_image;
}

fn compare_mosaics(mos1: &RgbImage, mos2: &RgbImage) -> f64 {
    if &mos1.dimensions() != &mos2.dimensions() {
        println!(
            "Mosaics have different sizes: {:?},{:?}",
            &mos1.dimensions(),
            &mos2.dimensions()
        );
    }

    let (size_x, size_y) = &mos1.dimensions();

    let similarity_percent: f64;
    let mut pixel_count: i64 = 0;
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

            pixel_count += 1;
        }
    }

    similarity_percent = similarity_accumulator / pixel_count as f64;

    return similarity_percent;
}

fn cache_message(
    mut vec: Vec<UpdateWithCx<AutoSend<Bot>, Message>>,
    message: UpdateWithCx<AutoSend<Bot>, Message>,
) {
    vec.push(message);
}

async fn get_photos_from_message(
    message: &UpdateWithCx<AutoSend<Bot>, Message>,
) -> Result<Option<Vec<String>>, Box<dyn Error>> {
    let mut saved_images: Vec<String> = vec![];

    match message.update.photo() {
        Some(photos) => {
            let photo = photos.last().unwrap();
            let file_id = &photo.file_id;

            let TgFile {
                file_path,
                file_size,
                ..
            } = message.requester.get_file(file_id).send().await?;

            println!("{:?} {:?} / {:?}", file_path, file_size, photos.len());

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
    recents: &mut Vec<RgbImage>,
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

    recents.push(newmosaic);

    return false;
}

#[tokio::main]
async fn main() {
    teloxide::enable_logging!();
    log::info!("Starting dices_bot...");

    let mut recent_messages: Vec<UpdateWithCx<AutoSend<Bot>, Message>> = vec![];
    let recent_images = Arc::new(RwLock::new(Vec::new()));

    let bot = Bot::from_env().auto_send();

    teloxide::repl(bot, {
        move |message| {
            let recent_images = Arc::clone(&recent_images);
            async move {
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
                    } else {
                        message.answer("That's fresh.").await?;
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
