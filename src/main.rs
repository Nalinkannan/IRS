use dioxus::desktop::tao::window::Icon;
use dioxus::desktop::{Config, WindowBuilder};
use dioxus::events::KeyboardEvent;
use dioxus::prelude::*;
use image::GenericImageView;
use keyboard_types::Key;
use std::path::PathBuf;

const MAIN_CSS: Asset = asset!("/src/main.css");
const THUMBNAIL_SIZE: u32 = 200;


fn main() {
    let img = image::load_from_memory(include_bytes!("../assets/icon.png"))
        .expect("Failed to load icon")
        .into_rgba8();
    let (width, height) = img.dimensions();
    dioxus::LaunchBuilder::new()
        .with_cfg(
            Config::default()
                .with_window(
                WindowBuilder::new()
                    .with_title("IRS - IMAGE RENAME SPLIT")
                    .with_maximized(true),
            )
                .with_icon(Icon::from_rgba(img.to_vec(), width, height).unwrap())
                .with_menu(None)
        )
        .launch(App);
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ImageItem {
    id: usize,
    path: PathBuf,
    thumbnail_base64: String,
}

#[derive(Clone, Debug, PartialEq)]
struct Notification {
    message: String,
    notification_type: NotificationType,
    id: u64,
}

#[derive(Clone, Debug, PartialEq, Copy)]
enum NotificationType {
    Info,
    Success,
    Error,
    Processing,
}

#[component]
fn App() -> Element {
    let images = use_signal(|| Vec::<ImageItem>::new());
    let folder_path = use_signal(|| None::<PathBuf>);
    let processing = use_signal(|| false);
    let notification = use_signal(|| None::<Notification>);
    let loading_files = use_signal(|| false);
    let drag_source = use_signal(|| None::<usize>);
    let drag_over_id = use_signal(|| None::<usize>);

    rsx! {
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        Controls {
            images,
            folder_path,
            processing,
            notification,
            loading_files,
        }
        ImagePreview {
            images,
            drag_source,
            drag_over_id,
        }
        if loading_files() {
            LoadingPopup {}
        }
        if let Some(notif) = notification() {
            NotificationPopup {
                notification: notif,
            }
        }
    }
}

#[component]
fn LoadingPopup() -> Element {
    rsx! {
        div {
            id: "loading-overlay",
            div {
                class: "loading-card",
                div {
                    class: "spinner",
                }
                p {
                    "Loading images..."
                }
            }
        }
    }
}

#[component]
fn NotificationPopup(notification: Notification) -> Element {
    let class_name = match notification.notification_type {
        NotificationType::Info => "notification-info",
        NotificationType::Success => "notification-success",
        NotificationType::Error => "notification-error",
        NotificationType::Processing => "notification-processing",
    };

    rsx! {
        div {
            id: "notification-overlay",
            div {
                class: "notification-card {class_name}",
                if notification.notification_type == NotificationType::Processing {
                    div {
                        class: "spinner",
                    }
                }
                p {
                    "{notification.message}"
                }
            }
        }
    }
}

#[component]
fn Controls(
    images: Signal<Vec<ImageItem>>,
    folder_path: Signal<Option<PathBuf>>,
    processing: Signal<bool>,
    mut notification: Signal<Option<Notification>>,
    mut loading_files: Signal<bool>,
) -> Element {
    let mut show_notification = move |message: String, notification_type: NotificationType| {
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        notification.set(Some(Notification {
            message,
            notification_type,
            id,
        }));

        if notification_type != NotificationType::Processing {
            spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                notification.set(None);
            });
        }
    };

    let open_files = move |_| {
        loading_files.set(true);

        spawn({
            async move {
                match rfd::AsyncFileDialog::new()
                    .add_filter("jpg", &["jpg", "jpeg"])
                    .pick_files()
                    .await
                {
                    Some(paths) if !paths.is_empty() => {
                        let folder = paths[0]
                            .path()
                            .parent()
                            .map(|p| p.to_path_buf())
                            .unwrap_or_else(|| PathBuf::from("."));
                        folder_path.set(Some(folder));

                        let file_paths: Vec<PathBuf> =
                            paths.iter().map(|p| p.path().to_path_buf()).collect();

                        let total_files = file_paths.len();

                        tokio::task::spawn_blocking(move || {
                            let mut image_items = Vec::new();
                            let mut id = 0;

                            for path_buf in file_paths {
                                match create_thumbnail(&path_buf) {
                                    Ok(thumbnail_base64) => {
                                        image_items.push(ImageItem {
                                            id,
                                            path: path_buf,
                                            thumbnail_base64,
                                        });
                                        id += 1;
                                    }
                                    Err(_) => {}
                                }
                            }

                            (image_items, total_files)
                        })
                        .await
                        .ok()
                        .map(|(image_items, _)| {
                            if !image_items.is_empty() {
                                images.set(image_items.clone());
                                show_notification(
                                    format!("✓ Loaded {} images", image_items.len()),
                                    NotificationType::Success,
                                );
                            } else {
                                show_notification(
                                    "✗ No valid images found".to_string(),
                                    NotificationType::Error,
                                );
                            }
                        });
                    }
                    _ => {
                        show_notification("No files selected".to_string(), NotificationType::Info);
                    }
                }
                loading_files.set(false);
            }
        });
    };

    let clear_images = move |_| {
        images.set(Vec::new());
        folder_path.set(None);
        show_notification("Cleared all images".to_string(), NotificationType::Info);
    };

    let rename_split = move |_| {
        if images().is_empty() {
            show_notification("No images to process".to_string(), NotificationType::Error);
            return;
        }

        processing.set(true);
        show_notification(
            "Selecting save location...".to_string(),
            NotificationType::Info,
        );

        let imgs = images.read().clone();

        spawn({
            async move {
                match rfd::AsyncFileDialog::new()
                    .set_title("Select folder to save split images")
                    .pick_folder()
                    .await
                {
                    Some(folder_handle) => {
                        let save_folder = folder_handle.path().to_path_buf();

                        // Notify user that processing is starting (processing popup)
                        show_notification(
                            "Processing images...".to_string(),
                            NotificationType::Processing,
                        );

                        // Run CPU-bound processing on a blocking thread but await it here so we can update UI safely.
                        // This prevents the UI from freezing while still allowing us to set notifications after completion.
                        let imgs_for_bg = imgs.clone();
                        match tokio::task::spawn_blocking(move || {
                            process_images_sync(imgs_for_bg, save_folder)
                        })
                        .await
                        {
                            Ok(Ok(processed_count)) => {
                                show_notification(
                                    format!("✓ Completed! Processed {} images", processed_count),
                                    NotificationType::Success,
                                );
                            }
                            Ok(Err(err_msg)) => {
                                show_notification(
                                    format!("✗ Processing error: {}", err_msg),
                                    NotificationType::Error,
                                );
                            }
                            Err(join_err) => {
                                show_notification(
                                    format!("✗ Processing task failed: {}", join_err),
                                    NotificationType::Error,
                                );
                            }
                        }

                        // Ensure processing flag is cleared
                        processing.set(false);
                    }
                    None => {
                        show_notification(
                            "Save location cancelled".to_string(),
                            NotificationType::Info,
                        );
                        processing.set(false);
                    }
                }
            }
        });
    };

    rsx! {
        div {
            id: "controls",
            button {
                id: "open-button",
                onclick: open_files,
                disabled: processing() || loading_files(),
                "OPEN"
            }
            button {
                id: "clear-button",
                onclick: clear_images,
                disabled: processing() || loading_files(),
                "CLEAR"
            }
            button {
                id: "rename-split-button",
                onclick: rename_split,
                disabled: processing() || loading_files(),
                "RENAME & SPLIT"
            }
        }
    }
}

#[component]
fn ImagePreview(
    images: Signal<Vec<ImageItem>>,
    drag_source: Signal<Option<usize>>,
    drag_over_id: Signal<Option<usize>>,
) -> Element {
    rsx! {
        div {
            id: "image-preview",
            if images().is_empty() {
                div {
                    class: "empty-preview",
                    "No images loaded. Click OPEN to select JPG files."
                }
            } else {
                for (idx, item) in images().iter().enumerate() {
                    ImageCard {
                        key: "{idx}-{item.id}",
                        item: item.clone(),
                        drag_source,
                        drag_over_id,
                        images,
                    }
                }
            }
        }
    }
}

#[component]
fn ImageCard(
    item: ImageItem,
    drag_source: Signal<Option<usize>>,
    drag_over_id: Signal<Option<usize>>,
    images: Signal<Vec<ImageItem>>,
) -> Element {
    let item_id = item.id;
    let is_drag_over = drag_over_id() == Some(item_id);
    let thumbnail = item.thumbnail_base64.clone();
    let item_name = item
        .path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    // Handler to move the item left (earlier in the list)
    let move_left = {
        let mut images = images.clone();
        move |_| {
            let mut imgs = images.read().clone();
            if let Some(idx) = imgs.iter().position(|img| img.id == item_id) {
                if idx > 0 {
                    imgs.swap(idx, idx - 1);
                    images.set(imgs);
                }
            }
        }
    };

    // Handler to move the item right (later in the list)
    let move_right = {
        let mut images = images.clone();
        move |_| {
            let mut imgs = images.read().clone();
            if let Some(idx) = imgs.iter().position(|img| img.id == item_id) {
                if idx + 1 < imgs.len() {
                    imgs.swap(idx, idx + 1);
                    images.set(imgs);
                }
            }
        }
    };

    rsx! {
        div {
            class: "image-item",
            class: if is_drag_over { "drag-over" } else { "" },
            draggable: true,
            tabindex: "0",
            onkeydown: move |evt: KeyboardEvent| {
                // Use Arrow keys to reorder focused image card
                match evt.key() {
                    Key::ArrowLeft => {
                        let mut imgs = images.read().clone();
                        if let Some(idx) = imgs.iter().position(|img| img.id == item_id) {
                            if idx > 0 {
                                imgs.swap(idx, idx - 1);
                                images.set(imgs);
                            }
                        }
                    }
                    Key::ArrowRight => {
                        let mut imgs = images.read().clone();
                        if let Some(idx) = imgs.iter().position(|img| img.id == item_id) {
                            if idx + 1 < imgs.len() {
                                imgs.swap(idx, idx + 1);
                                images.set(imgs);
                            }
                        }
                    }
                    _ => {}
                }
            },
            ondragstart: move |_| {
                drag_source.set(Some(item_id));
            },
            ondragover: move |evt: DragEvent| {
                evt.prevent_default();
                drag_over_id.set(Some(item_id));
            },
            ondrop: move |evt: DragEvent| {
                evt.prevent_default();

                if let Some(source_id) = drag_source() {
                    if source_id != item_id {
                        let mut imgs = images.read().clone();

                        let source_idx = imgs.iter().position(|img| img.id == source_id);
                        let target_idx = imgs.iter().position(|img| img.id == item_id);

                        if let (Some(src), Some(tgt)) = (source_idx, target_idx) {
                            imgs.swap(src, tgt);
                            images.set(imgs);
                        }
                    }
                }
                drag_source.set(None);
                drag_over_id.set(None);
            },
            ondragleave: move |_| {
                drag_over_id.set(None);
            },

            // Control row with SVG arrows
            div {
                class: "move-buttons",
                // Left arrow button (SVG)
                button {
                    onclick: move_left,
                    title: "Move left",
                    aria_label: "Move left",
                    svg {
                        xmlns: "http://www.w3.org/2000/svg",
                        view_box: "0 0 24 24",
                        width: "14",
                        height: "14",
                        fill: "white",
                        path {
                            d: "M15.41 7.41L14 6l-6 6 6 6 1.41-1.41L10.83 12z"
                        }
                    }
                }
                // Right arrow button (SVG)
                button {
                    onclick: move_right,
                    title: "Move right",
                    aria_label: "Move right",
                    fill: "white",
                    svg {
                        xmlns: "http://www.w3.org/2000/svg",
                        view_box: "0 0 24 24",
                        width: "14",
                        height: "14",
                        path {
                            d: "M8.59 16.59L10 18l6-6-6-6-1.41 1.41L13.17 12z"
                        }
                    }
                }
            }

            img {
                src: "data:image/jpeg;base64,{thumbnail}",
                alt: "Preview",
            }
            div {
                class: "image-label",
                "{item_name}"
            }
        }
    }
}

fn create_thumbnail(path: &PathBuf) -> Result<String, Box<dyn std::error::Error>> {
    let img = image::open(path)?;
    let thumbnail = img.thumbnail(THUMBNAIL_SIZE, THUMBNAIL_SIZE);
    let rgb_img = thumbnail.to_rgb8();

    let mut jpg_data = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg_data, 85);
    encoder.encode_image(&rgb_img)?;

    let base64_str = encode_to_base64(&jpg_data)?;
    Ok(base64_str)
}

fn encode_to_base64(data: &[u8]) -> Result<String, Box<dyn std::error::Error>> {
    let mut result = String::new();
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    for chunk in data.chunks(3) {
        let b1 = chunk[0];
        let b2 = chunk.get(1).copied().unwrap_or(0);
        let b3 = chunk.get(2).copied().unwrap_or(0);

        let n = ((b1 as u32) << 16) | ((b2 as u32) << 8) | (b3 as u32);

        result.push(TABLE[((n >> 18) & 63) as usize] as char);
        result.push(TABLE[((n >> 12) & 63) as usize] as char);

        if chunk.len() > 1 {
            result.push(TABLE[((n >> 6) & 63) as usize] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(TABLE[(n & 63) as usize] as char);
        } else {
            result.push('=');
        }
    }

    Ok(result)
}

fn process_single_image(
    item: &ImageItem,
    spl_folder: &PathBuf,
    sequence_num: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let img = image::open(&item.path)?;

    let (width, height) = img.dimensions();
    let half_width = width / 2;

    let left_half = img.crop_imm(0, 0, half_width, height);
    let right_half = img.crop_imm(half_width, 0, half_width, height);

    let left_path = spl_folder.join(format!("{}_{}.jpg", pad_number(sequence_num), "1"));
    let right_path = spl_folder.join(format!("{}_{}.jpg", pad_number(sequence_num), "2"));

    save_with_dpi(&left_half, &left_path, 100)?;
    save_with_dpi(&right_half, &right_path, 100)?;

    Ok(())
}

fn save_with_dpi(
    img: &image::DynamicImage,
    path: &PathBuf,
    quality: u8,
) -> Result<(), Box<dyn std::error::Error>> {
    // Encode image into an in-memory JPEG buffer first
    let mut jpg_buf: Vec<u8> = Vec::new();
    {
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg_buf, quality);
        let rgb_image = img.to_rgb8();
        encoder.encode_image(&rgb_image)?;
    }

    // Ensure JFIF APP0 segment sets DPI (units = inch, X/Y density)
    set_jpeg_dpi(&mut jpg_buf, 300)?;

    // Write bytes to file
    std::fs::write(path, &jpg_buf)?;
    Ok(())
}

// Find JFIF APP0 segment and set units and X/Y density. If not present, insert one after SOI.
fn set_jpeg_dpi(buf: &mut Vec<u8>, dpi: u16) -> Result<(), Box<dyn std::error::Error>> {
    // Validate JPEG SOI
    if buf.len() < 4 || buf[0] != 0xFF || buf[1] != 0xD8 {
        return Err("Not a valid JPEG".into());
    }

    // Walk segments starting after SOI
    let mut i = 2usize; // start after SOI
    while i + 4 <= buf.len() {
        if buf[i] != 0xFF {
            break;
        }
        let marker = buf.get(i + 1).copied().unwrap_or(0);
        // Start of Scan (0xDA) — image data starts, stop searching
        if marker == 0xDA {
            break;
        }
        // Ensure we can read length
        if i + 4 > buf.len() {
            break;
        }
        let len = ((buf[i + 2] as usize) << 8) | (buf[i + 3] as usize);
        if len < 2 {
            break;
        }
        // APP0 marker is 0xE0
        if marker == 0xE0 {
            // Check for "JFIF\0" identifier at i+4..i+9
            if i + 4 + 5 <= buf.len() {
                if &buf[i + 4..i + 9] == b"JFIF\0" {
                    // units at offset i+11, xdensity at i+12..13, ydensity at i+14..15
                    if i + 15 < buf.len() {
                        let units_pos = i + 11;
                        let x_pos = i + 12;
                        buf[units_pos] = 1; // dots per inch
                        buf[x_pos] = (dpi >> 8) as u8;
                        buf[x_pos + 1] = (dpi & 0xFF) as u8;
                        buf[x_pos + 2] = (dpi >> 8) as u8;
                        buf[x_pos + 3] = (dpi & 0xFF) as u8;
                        return Ok(());
                    }
                }
            }
        }
        // move to next segment: marker(2) + length bytes
        i += 2 + len;
    }

    // If no JFIF APP0 found — insert one right after SOI (offset 2)
    // Build APP0 JFIF segment (length = 16 -> 0x0010)
    let mut app0: Vec<u8> = Vec::new();
    app0.push(0xFF);
    app0.push(0xE0);
    app0.push(0x00);
    app0.push(0x10); // length 16
    app0.extend_from_slice(b"JFIF\0"); // identifier
    app0.push(0x01); // version major
    app0.push(0x02); // version minor
    app0.push(0x01); // units = dots per inch
    app0.push((dpi >> 8) as u8);
    app0.push((dpi & 0xFF) as u8);
    app0.push((dpi >> 8) as u8);
    app0.push((dpi & 0xFF) as u8);
    app0.push(0x00); // Xthumbnail
    app0.push(0x00); // Ythumbnail

    // Insert after SOI (position 2)
    buf.splice(2..2, app0.iter().cloned());

    Ok(())
}

fn process_images_sync(images: Vec<ImageItem>, save_folder: PathBuf) -> Result<usize, String> {
    // Synchronous version of the threaded processing. Returns number of images processed or Err(msg).
    let spl_folder = save_folder.join("SPL");
    if let Err(e) = std::fs::create_dir_all(&spl_folder) {
        return Err(format!("Failed to create output folder: {}", e));
    }

    let (tx, rx) = std::sync::mpsc::channel();
    let images_arc = std::sync::Arc::new(images);
    let spl_folder_arc = std::sync::Arc::new(spl_folder);
    let mut handles = Vec::new();

    let chunk_size = 3;
    let total_images = images_arc.len();
    let mut image_num = 1usize;

    for chunk in images_arc.chunks(chunk_size) {
        let chunk_clone = chunk.to_vec();
        let chunk_len = chunk_clone.len();
        let tx = tx.clone();
        let spl_folder = std::sync::Arc::clone(&spl_folder_arc);
        let start_num = image_num;

        let handle = std::thread::spawn(move || {
            for (idx, item) in chunk_clone.iter().enumerate() {
                let current_num = start_num + idx;
                match process_single_image(item, &spl_folder, current_num) {
                    Ok(_) => {
                        let _ = tx.send(format!(
                            "✓ {}",
                            item.path.file_name().unwrap_or_default().to_string_lossy()
                        ));
                    }
                    Err(_) => {
                        let _ = tx.send(format!(
                            "✗ {}",
                            item.path.file_name().unwrap_or_default().to_string_lossy()
                        ));
                    }
                }
            }
        });

        image_num += chunk_len;
        handles.push(handle);
    }

    drop(tx);

    // Collect results (this will block until all senders are dropped)
    let mut _results: Vec<String> = Vec::new();
    for msg in rx.iter() {
        _results.push(msg);
    }

    // Join threads
    for handle in handles {
        let _ = handle.join();
    }

    Ok(total_images)
}

fn pad_number(num: usize) -> String {
    format!("{:02}", num)
}
