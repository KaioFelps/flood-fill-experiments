use image::{DynamicImage, GenericImage, GenericImageView, Luma, Pixel, Rgba};
use std::{
    collections::{BTreeSet, HashSet, VecDeque},
    thread::{JoinHandle, spawn},
};

const THRESHOLD: u8 = 5;
const EUCLIDEAN_DISTANCE_THRESHOLD: f64 = 0.030;
// weights for lightning, a, and b oklab channels
// obtained from many attempts...
const OKLAB_WEIGHTS: [f64; 3] = [0.7, 1.0, 1.0];

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
    let images = [
        ["imagem 3.png", "imagem 3.out.jpg"],
        ["teste.jpg", "teste.out.jpg"],
        ["image 2.webp", "image 2.out.jpg"],
    ];

    let mut handlers = Vec::<JoinHandle<()>>::new();

    for image in images {
        println!("Spawnando pra imagem \"{}\".", image[0]);

        let handle = spawn(move || {
            segment(image[0], image[1]);
        });

        handlers.push(handle);
    }

    for (index, handler) in handlers.into_iter().enumerate() {
        if let Err(err) = handler.join() {
            println!(
                "processamento pra imagem \"{}\" deu erro: {err:#?}",
                images[index][0]
            );
        }
    }
}

fn segment(input: &str, output: &str) {
    let image = image::open(input).expect("falha ao abrir a imagem");
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
    let total_segments = segments.len().max(1);
    for (index, segment) in segments.into_iter().enumerate() {
        let intensity = ((index * 255) / total_segments) as u8;
        for (x, y) in segment {
            segments_image.put_pixel(x, y, Luma([intensity]).to_rgba());
        }
    }
    segments_image
        .save(output)
        .expect("Não foi possível salvar a imagem.");
}

fn to_oklab(pixel: &Rgba<u8>) -> [f64; 3] {
    let mut channels: [f64; 3] = [0.0, 0.0, 0.0];
    let rgb_channels = pixel.0;
    for i in 0..3 {
        let channel = rgb_channels[i];
        let normalized_val = channel as f32 / 255.0;
        let clinear = if normalized_val <= 0.04045 {
            normalized_val as f64 / 12.92f64
        } else {
            ((normalized_val as f64 + 0.055f64) / 1.055f64).powf(2.4f64)
        };
        channels[i] = clinear;
    }

    let l_space =
        0.4122214708 * channels[0] + 0.5363325363 * channels[1] + 0.0514459929 * channels[2];
    let m_space =
        0.2119034982 * channels[0] + 0.6806995451 * channels[1] + 0.1073969566 * channels[2];
    let s_space =
        0.0883024619 * channels[0] + 0.2817188376 * channels[1] + 0.6299787005 * channels[2];

    let l_root = l_space.signum() * l_space.abs().powf(1.0 / 3.0);
    let m_root = m_space.signum() * m_space.abs().powf(1.0 / 3.0);
    let s_root = s_space.signum() * s_space.abs().powf(1.0 / 3.0);

    let lightness = 0.2104542553 * l_root + 0.7936177850 * m_root - 0.0040720468 * s_root;
    let a = 1.9779984951 * l_root - 2.4285922050 * m_root + 0.4505937099 * s_root;
    let b = 0.0259040371 * l_root + 0.7827717662 * m_root - 0.8086757660 * s_root;

    [lightness, a, b]
}

fn are_pixels_close(a: &Rgba<u8>, b: &Rgba<u8>) -> bool {
    let a = to_oklab(a);
    let b = to_oklab(b);

    let euclidean_distance = (OKLAB_WEIGHTS[0] * (a[0] - b[0]).powi(2)
        + OKLAB_WEIGHTS[1] * (a[1] - b[1]).powi(2)
        + OKLAB_WEIGHTS[2] * (a[2] - b[2]).powi(2))
    .sqrt();

    euclidean_distance <= EUCLIDEAN_DISTANCE_THRESHOLD
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
                    let are_similar = are_pixels_close(
                        &image.get_pixel(prev_pixel.0, prev_pixel.1).to_rgba(),
                        &image.get_pixel(pixel.0, pixel.1).to_rgba(),
                    );

                    if are_similar && !visited_pixels.contains(&pixel) {
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
