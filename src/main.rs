use image::{DynamicImage, GenericImage, GenericImageView, Luma, Pixel};
use std::{
    collections::{BTreeSet, HashSet, VecDeque},
    println,
};

const THRESHOLD: u8 = 25;

#[derive(Clone)]
enum DirectionToFollow {
    Above,
    Down,
    Left,
    Right,
}

impl DirectionToFollow {
    pub fn new() -> Self {
        Self::Above
    }

    pub fn next(self) -> Option<Self> {
        match self {
            Self::Above => Some(Self::Right),
            Self::Right => Some(Self::Down),
            Self::Down => Some(Self::Left),
            Self::Left => None,
        }
    }
}

fn main() {
    let image = image::open("image 2.webp").expect("falha ao abrir a imagem");

    // primeiro vamos jogar sementes na imagem. Pra isso, vou "dividir" a imagem em 9 blocos e
    // colocar uma semente no centro de cada um desses blocos.
    // cada semente está em uma layer diferente, então, posteriormente, sementes que cobrirem
    // as mesmas áreas serão unificadas.

    let column_width = image.width() / 3;
    let row_height = image.height() / 3;

    // A vector of (col, row) tuples. These are the seeds.
    let seeds = [0, 1, 2]
        .into_iter()
        .flat_map(|col| [0, 1, 2].map(|row| generate_median(col, row, column_width, row_height)))
        .collect::<Vec<_>>();

    // The layer of segments that will expand from seeds.
    let mut segments = HashSet::<BTreeSet<(u32, u32)>>::new();
    // The maximum difference of intensity between a pixel and another for them to be considered neighbors.

    // a bitmap of visited pixels. Pixels will be removed from this hashset as they're visited so that
    // regions not covered by seeds can be visited, finally checking the entire image.
    let mut missing_pixels = (0..image.width())
        .flat_map(|col| (0..image.height()).map(move |row| (col, row)))
        .collect::<HashSet<_>>();

    // Each seed could grow in separate threads, but I don't wanna deal with this right now. Hence, simple loop.
    for seed in seeds {
        println!("Expandindo a partir da seed ({}, {}).", seed.0, seed.1);
        // now we perform a depth-first search to visit every pixel that has an intensity close to the `seed`'s.
        let mut visited_pixels = HashSet::<(u32, u32)>::new();

        explore_axis_non_recursively(seed, &mut visited_pixels, &mut missing_pixels, &image);

        visited_pixels.insert(seed);
        segments.insert(visited_pixels.into_iter().collect::<BTreeSet<_>>());
    }

    // Now we're creating new segments from pixels that hasn't been visited yet by expanding the initial seeds.
    println!("Iniciando a expansão pelos pixels não visitados.");
    while let Some(pixel) = missing_pixels.iter().next().cloned() {
        println!(
            "Expandindo a partir do pixel não visitado ({}, {}).",
            pixel.0, pixel.1
        );

        let mut visited_pixels = HashSet::<(u32, u32)>::new();

        explore_axis_non_recursively(pixel, &mut visited_pixels, &mut missing_pixels, &image);

        visited_pixels.insert(pixel);
        segments.insert(visited_pixels.into_iter().collect::<BTreeSet<_>>());
    }

    let mut segments_image = DynamicImage::new_luma8(image.width(), image.height());
    let intensity_slice = 255u8 / segments.len() as u8;
    for (index, segment) in segments.into_iter().enumerate() {
        let intensity = (index * intensity_slice as usize) as u8;
        for (x, y) in segment {
            segments_image.put_pixel(x, y, Luma([intensity]).to_rgba());
        }
    }
    segments_image
        .save("resultado.jpg")
        .expect("Não foi possível salvar a imagem.");
}

fn explore_axis_non_recursively(
    initial_pos: (u32, u32),
    visited_pixels: &mut HashSet<(u32, u32)>,
    missing_pixels: &mut HashSet<(u32, u32)>,
    image: &DynamicImage,
) {
    let mut queue = VecDeque::<(u32, u32)>::new();
    queue.push_back(initial_pos);

    while let Some(prev_pixel) = queue.pop_front() {
        if !visited_pixels.contains(&prev_pixel) {
            let mut direction = Some(DirectionToFollow::new());
            while let Some(new_direction) = direction {
                let next_pixel = match new_direction {
                    DirectionToFollow::Above => (prev_pixel.0 as i64, prev_pixel.1 as i64 - 1),
                    DirectionToFollow::Down => (prev_pixel.0 as i64, prev_pixel.1 as i64 + 1),
                    DirectionToFollow::Right => (prev_pixel.0 as i64 + 1, prev_pixel.1 as i64),
                    DirectionToFollow::Left => (prev_pixel.0 as i64 - 1, prev_pixel.1 as i64),
                };

                let pixel_is_out_of_bounds = next_pixel.0 < 0
                    || next_pixel.0 >= image.width() as i64
                    || next_pixel.1 < 0
                    || next_pixel.1 >= image.height() as i64;

                if !pixel_is_out_of_bounds {
                    let pixel = (next_pixel.0 as u32, next_pixel.1 as u32);
                    let previous_pixel_intensity =
                        image.get_pixel(prev_pixel.0, prev_pixel.1).to_luma().0[0];
                    let current_pixel_intensity = image.get_pixel(pixel.0, pixel.1).to_luma().0[0];

                    if previous_pixel_intensity.abs_diff(current_pixel_intensity) <= THRESHOLD
                        && !visited_pixels.contains(&pixel)
                    {
                        queue.push_back(pixel);
                    }
                }

                direction = new_direction.next();
            }
        }

        visited_pixels.insert(prev_pixel);
        missing_pixels.remove(&prev_pixel);
    }
}

fn explore_axis(
    prev_pos: Option<(u32, u32)>,
    current_pos: (u32, u32),
    visited_pixels: &mut HashSet<(u32, u32)>,
    image: &DynamicImage,
) {
    if visited_pixels.contains(&current_pos) {
        return;
    }

    if let Some(prev) = prev_pos {
        let previous_pixel_intensity = image.get_pixel(prev.0, prev.1).to_luma().0[0];
        let current_pixel_intensity = image.get_pixel(current_pos.0, current_pos.1).to_luma().0[0];

        if previous_pixel_intensity.abs_diff(current_pixel_intensity) >= THRESHOLD {
            return;
        }
    }

    visited_pixels.insert(current_pos);

    let mut direction = Some(DirectionToFollow::new());
    while let Some(new_direction) = direction {
        let next_pixel = match new_direction {
            DirectionToFollow::Above => (current_pos.0 as i64, current_pos.1 as i64 - 1),
            DirectionToFollow::Down => (current_pos.0 as i64, current_pos.1 as i64 + 1),
            DirectionToFollow::Right => (current_pos.0 as i64 + 1, current_pos.1 as i64),
            DirectionToFollow::Left => (current_pos.0 as i64 - 1, current_pos.1 as i64),
        };

        let pixel_is_out_of_bounds = next_pixel.0 < 0
            || next_pixel.0 >= image.width() as i64
            || next_pixel.1 < 0
            || next_pixel.1 >= image.height() as i64;

        if !pixel_is_out_of_bounds {
            explore_axis(
                Some(current_pos),
                (next_pixel.0 as u32, next_pixel.1 as u32),
                visited_pixels,
                image,
            );
        }

        direction = new_direction.next();
    }
}

/// Resolves the median pixel of a region of the image. 0-indexed.
fn generate_median(col: u8, row: u8, region_width: u32, region_height: u32) -> (u32, u32) {
    let region_col = (col as u32 * region_width + (col as u32 + 1u32) * region_width) / 2;
    let region_row = (row as u32 * region_height + (row as u32 + 1u32) * region_height) / 2;
    (region_col, region_row)
}
