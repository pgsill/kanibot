extern crate image;
use image::io::Reader as ImageReader;
use image::{open, GenericImage, GenericImageView, ImageBuffer, RgbImage};
use std::error::Error;
use std::path::Path;
use std::{env, process};
use teloxide::{
    net::Download,
    requests::{Request, Requester},
    types::File as TgFile,
    Bot,
};
use teloxide::{prelude::*, types::PhotoSize, RequestError};
use tokio::fs::File;

fn distance3d(set1: [u8; 3], set2: [u8; 3]) -> f64 {
    let [x1, y1, z1] = set1;
    let [x2, y2, z2] = set2;

    let power = (x1 - x2).pow(2) + (y1 - y2).pow(2) + (z1 - z2).pow(2);

    return f64::sqrt(power as f64);
}

fn get_belonging_third(position: u32, size: u32) -> u32 {
    if position <= size {
        return 0;
    } else if position > size && position < size * 2 {
        return 1;
    } else {
        return 2;
    }
}

fn make_3x3_mosaic(filename: &std::string::String) -> RgbImage {
    match image::open(filename) {
        Ok(img) => {
            // The dimensions method returns the images width and height.
            println!("dimensions {:?}", img.dimensions());
            // The color method returns the image's `ColorType`.
            println!("{:?}", img.color());

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
                );
            }

            new_image
                .save(format!("resultfrom{}.png", filename))
                .unwrap();
            return new_image;
        }
        Err(e) => {
            println!("Couldn't open file {:?}: {:?}", filename, e);
            process::exit(0x0100);
        }
    }
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

    let mut similarity_percent: f64 = 0.0;

    for pos_x in 0..*size_x {
        for pos_y in 0..*size_y {
            let [mos1_r, mos1_g, mos1_b] = &mos1.get_pixel(pos_x, pos_y).0;
            let [mos2_r, mos2_g, mos2_b] = &mos2.get_pixel(pos_x, pos_y).0;

            let difference = ((*mos2_r as i32 - *mos1_r as i32).pow(2)
                + (*mos2_g as i32 - *mos1_g as i32).pow(2)
                + (*mos2_b as i32 - *mos1_b as i32).pow(2)) as f64;
            let current_similarity_percentage =
                difference / ((255u32.pow(2) + 255u32.pow(2) + 255u32.pow(2)) as f64).sqrt();

            println!(
                "percentage for pixel {:?},{:?}: {:.2}%, {:.2} absolute difference. values are: [R: {:?},{:?}][G: {:?},{:?}][B: {:?},{:?}] ",
                pos_x, pos_y, current_similarity_percentage, difference,
                mos2_r,mos1_r,
                mos2_g,mos1_g,
                mos2_b,mos1_b
            );

            similarity_percent = (current_similarity_percentage + similarity_percent) / 2.0;
        }
    }

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
            for photo in photos {
                let file_id = &photo.file_id;
                let image_filename = format!("{}.png", file_id);

                let TgFile { file_path, .. } = message.requester.get_file(file_id).send().await?;

                let mut file = File::create(&image_filename).await?;

                message
                    .requester
                    .download_file(&file_path, &mut file)
                    .await?;

                saved_images.push(image_filename);
            }

            return Ok(Some(saved_images));
        }
        None => Ok(None),
    }
}

fn get_similar_image_posted_recently(image: RgbImage, recents: &Vec<RgbImage>) -> bool {
    for recentImage in recents {
        if compare_mosaics(&image, recentImage) > 1000.0 {
            return true;
        }
    }

    return false;
}

#[tokio::main]
async fn main() {
    teloxide::enable_logging!();
    log::info!("Starting dices_bot...");

    let mut recent_messages: Vec<UpdateWithCx<AutoSend<Bot>, Message>> = vec![];
    let mut recent_images: Vec<RgbImage> = vec![];
    let x: Box<Vec<RgbImage>> = Box::new(recent_images);
    let static_ref: &'static mut Vec<RgbImage> = Box::leak(x);

    let bot = Bot::from_env().auto_send();

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
    .await;
}
