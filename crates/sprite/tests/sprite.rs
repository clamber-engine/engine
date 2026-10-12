use std::path::PathBuf;

use clamber_sprite::{
    AreaKind, AtlasBuilder, Image, Palette, Pixels, Rect, Sheet, SpriteSheet, decode_png,
};

fn walker() -> Sheet {
    Sheet::from_json_str(include_str!("fixtures/walker.json"), "walker.json").unwrap()
}

fn sheet_err(src: &str) -> String {
    Sheet::from_json_str(src, "s.json").unwrap_err().to_string()
}

/// A minimal valid sheet with `frames` and `meta` extras spliced in.
fn sheet_with(frames: &str, meta: &str) -> String {
    format!(r#"{{ "frames": [{frames}], "meta": {{ "image": "x.png" {meta} }} }}"#)
}

const FRAME: &str = r#"{ "frame": { "x": 0, "y": 0, "w": 8, "h": 8 }, "duration": 100 }"#;

/// Encodes an indexed PNG at the given bit depth.
fn indexed_png(width: u32, height: u32, depth: png::BitDepth, rows: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, width, height);
    enc.set_color(png::ColorType::Indexed);
    enc.set_depth(depth);
    enc.set_palette(vec![0, 0, 0, 0x20, 0x30, 0x40, 0xe0, 0xc0, 0x80]);
    enc.set_trns(vec![0]);
    let mut w = enc.write_header().unwrap();
    w.write_image_data(&rows.concat()).unwrap();
    w.finish().unwrap();
    out
}

#[test]
fn frames_durations_and_trim() {
    let s = walker();
    assert_eq!(s.image, "walker.png");
    let ticks: Vec<u32> = s.frames.iter().map(|f| f.ticks).collect();
    // 100 ms, 50 ms, explicit ticks, 17 ms
    assert_eq!(ticks, [6, 3, 5, 1]);
    assert_eq!(
        s.frames[3].rect,
        Rect {
            x: 48,
            y: 0,
            w: 12,
            h: 14
        }
    );
    // origin is the pivot at (8, 16); frame 3 was trimmed by (2, 2)
    assert_eq!(s.frames[0].offset, (-8, -16));
    assert_eq!(s.frames[3].offset, (-6, -14));
}

#[test]
fn slices_become_areas_per_frame() {
    let s = walker();
    let names = |i: usize| -> Vec<&str> { s.frames[i].boxes.iter().map(|b| &*b.name).collect() };
    assert_eq!(names(0), ["hurtbox"]);
    assert_eq!(names(1), ["hurtbox", "muzzle"]);
    assert_eq!(names(2), ["hurtbox", "hitbox-kick", "muzzle"]);
    // the empty key on frame 3 removes the hitbox
    assert_eq!(names(3), ["hurtbox", "muzzle"]);

    let hurt = &s.frames[0].boxes[0];
    assert_eq!(hurt.kind, AreaKind::Hurtbox);
    assert_eq!((hurt.x, hurt.y, hurt.w, hurt.h), (-4, -14, 8, 14));
    let kick = &s.frames[2].boxes[1];
    assert_eq!(kick.kind, AreaKind::Hitbox);
    assert_eq!((kick.x, kick.y), (4, -8));
    assert_eq!(s.frames[1].boxes[1].kind, AreaKind::Other);
}

#[test]
fn animation_directions_and_repeat() {
    let s = walker();
    let anim = |n: &str| s.animation(n).unwrap();
    assert_eq!(anim("walk").frames, [0, 1, 2, 3]);
    assert_eq!(anim("walk").repeat, None);
    assert_eq!(anim("bob").frames, [1, 2, 3, 2]);
    assert_eq!(anim("back").frames, [2, 1, 0]);
    assert_eq!(anim("back").repeat, Some(2));
    assert_eq!(anim("sway").frames, [2, 1, 0, 1]);
}

#[test]
fn palettes_from_metadata() {
    let s = walker();
    assert_eq!(s.palettes["red"].0[2], [0xff, 0x40, 0x40, 0xff]);
    assert_eq!(s.palettes["base"].0[0], [0, 0, 0, 0]);
}

#[test]
fn array_layout_and_default_origin() {
    let s = Sheet::from_json_str(include_str!("fixtures/array.json"), "array.json").unwrap();
    assert_eq!(s.frames.len(), 2);
    assert_eq!(s.frames[1].ticks, 60);
    assert_eq!(s.frames[1].offset, (-4, -8));
    assert_eq!(s.animation("blink").unwrap().frames, [1]);
}

#[test]
fn metadata_errors() {
    assert_eq!(sheet_err(&sheet_with("", "")), "s.json: frames: no frames");
    assert!(
        sheet_err(&sheet_with(
            r#"{ "frame": { "x": 0, "y": 0, "w": 8, "h": 8 } }"#,
            ""
        ))
        .contains("frame 0: no duration or ticks")
    );
    assert!(
        sheet_err(&sheet_with(
            r#"{ "frame": { "x": 0, "y": 0, "w": 8, "h": 8 }, "rotated": true, "duration": 1 }"#,
            ""
        ))
        .contains("rotated")
    );
    assert!(sheet_err(&sheet_with(FRAME, r#", "scale": "2""#)).contains("scale 2"));
    assert!(
        sheet_err(&sheet_with(FRAME, r#", "size": { "w": 4, "h": 4 }"#))
            .contains("outside the sheet image")
    );
    assert!(
        sheet_err(&sheet_with(
            FRAME,
            r#", "frameTags": [ { "name": "run", "from": 0, "to": 1 } ]"#
        ))
        .contains("frameTags.run: frames 0 to 1 aren't within the sheet's 1 frames")
    );
    assert!(
        sheet_err(&sheet_with(
            FRAME,
            r#", "frameTags": [ { "name": "run", "from": 0, "to": 0, "direction": "sideways" } ]"#
        ))
        .contains("unknown direction `sideways`")
    );
    assert!(
        sheet_err(&sheet_with(
            FRAME,
            r#", "slices": [ { "name": "hitbox", "keys": [ { "frame": 3, "bounds": { "x": 0, "y": 0, "w": 1, "h": 1 } } ] } ]"#
        ))
        .contains("slice hitbox: key on frame 3, but the sheet has 1 frames")
    );
    assert!(
        sheet_err(&sheet_with(
            FRAME,
            r##", "clamber": { "palettes": { "p": ["#12"] } }"##
        ))
        .contains("meta.clamber.palettes.p: bad color `#12`")
    );
    assert!(sheet_err(&sheet_with(FRAME, r#", "clamber": { "orign": {} }"#)).contains("orign"));
    assert!(
        sheet_err(&sheet_with(FRAME, r#", "clamber": { "scale": 0 }"#))
            .contains("meta.clamber.scale must be at least 1")
    );
}

#[test]
fn scale_and_pixel_art_default_to_plain_texels_filtered_smoothly() {
    let plain = Sheet::from_json_str(&sheet_with(FRAME, ""), "test").unwrap();
    assert_eq!((plain.scale, plain.pixel_art), (1, false));
    let hd = Sheet::from_json_str(
        &sheet_with(FRAME, r#", "clamber": { "scale": 5, "pixelArt": true }"#),
        "test",
    )
    .unwrap();
    assert_eq!((hd.scale, hd.pixel_art), (5, true));
}

#[test]
fn variants_name_images_and_not_palettes() {
    let s = Sheet::from_json_str(
        &sheet_with(
            FRAME,
            r#", "clamber": { "variants": { "alt": "x-alt.png" } }"#,
        ),
        "test",
    )
    .unwrap();
    assert_eq!(s.variants["alt"], "x-alt.png");
    assert!(
        sheet_err(&sheet_with(
            FRAME,
            r##", "clamber": { "palettes": { "alt": ["#000000"] }, "variants": { "alt": "x.png" } }"##
        ))
        .contains("meta.clamber.variants.alt: also the name of a palette")
    );
}

#[test]
fn indexed_png_keeps_indices_at_any_depth() {
    // 3x2 at 4 bits per pixel: rows are padded to whole bytes
    let bytes = indexed_png(
        3,
        2,
        png::BitDepth::Four,
        &[vec![0x12, 0x00], vec![0x21, 0x10]],
    );
    let Image::Indexed { indices, palette } = decode_png(&bytes).unwrap() else {
        panic!("expected indexed");
    };
    assert_eq!(indices.data, [1, 2, 0, 2, 1, 1]);
    assert_eq!(palette.0[0], [0, 0, 0, 0]);
    assert_eq!(palette.0[1], [0x20, 0x30, 0x40, 0xff]);

    let bytes = indexed_png(4, 1, png::BitDepth::Two, &[vec![0b00_01_10_01]]);
    let Image::Indexed { indices, .. } = decode_png(&bytes).unwrap() else {
        panic!("expected indexed");
    };
    assert_eq!(indices.data, [0, 1, 2, 1]);
}

#[test]
fn palette_swap_and_flash() {
    let s = walker();
    let indices = Pixels {
        width: 3,
        height: 1,
        data: vec![0, 1, 2],
    };
    let red = s.palettes["red"].apply(&indices);
    assert_eq!(red.data[2], [0xff, 0x40, 0x40, 0xff]);
    let white = s.palettes["red"]
        .flash([255, 255, 255, 255])
        .apply(&indices);
    assert_eq!(white.data, [[255, 255, 255, 0], [255; 4], [255; 4]]);
    // indices past the palette are transparent
    assert_eq!(Palette(vec![]).apply(&indices).data, [[0; 4]; 3]);
}

#[test]
fn loads_sheet_and_image_from_disk() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("sprite-load");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("walker.json"),
        include_str!("fixtures/walker.json"),
    )
    .unwrap();
    let rows: Vec<Vec<u8>> = (0..16).map(|y| vec![(y % 3) as u8; 64]).collect();
    std::fs::write(
        dir.join("walker.png"),
        indexed_png(64, 16, png::BitDepth::Eight, &rows),
    )
    .unwrap();

    let loaded = SpriteSheet::load(&dir.join("walker.json")).unwrap();
    assert_eq!(loaded.sheet.frames.len(), 4);
    assert!(matches!(loaded.image, Image::Indexed { .. }));

    // a missing image names the image path
    std::fs::write(
        dir.join("lost.json"),
        include_str!("fixtures/walker.json").replace("walker.png", "lost.png"),
    )
    .unwrap();
    let err = SpriteSheet::load(&dir.join("lost.json")).unwrap_err();
    assert!(err.path.ends_with("lost.png"), "{err}");

    // an image smaller than the frames is rejected
    std::fs::write(
        dir.join("walker.png"),
        indexed_png(32, 16, png::BitDepth::Eight, &vec![vec![0; 32]; 16]),
    )
    .unwrap();
    let err = SpriteSheet::load(&dir.join("walker.json")).unwrap_err();
    assert!(
        err.message.contains("frame 2 is outside the 32x16 image"),
        "{err}"
    );
}

#[test]
fn loads_variant_images_the_size_of_the_sheet() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("sprite-variants");
    std::fs::create_dir_all(&dir).unwrap();
    let json = include_str!("fixtures/walker.json").replace(
        r#""clamber": {"#,
        r#""clamber": { "variants": { "night": "night.png" },"#,
    );
    std::fs::write(dir.join("walker.json"), &json).unwrap();
    let rows: Vec<Vec<u8>> = (0..16).map(|y| vec![(y % 3) as u8; 64]).collect();
    std::fs::write(
        dir.join("walker.png"),
        indexed_png(64, 16, png::BitDepth::Eight, &rows),
    )
    .unwrap();
    std::fs::write(
        dir.join("night.png"),
        indexed_png(64, 16, png::BitDepth::Eight, &rows),
    )
    .unwrap();
    let loaded = SpriteSheet::load(&dir.join("walker.json")).unwrap();
    assert_eq!(loaded.variants.len(), 1);
    assert_eq!(loaded.variants["night"].width(), 64);

    std::fs::write(
        dir.join("night.png"),
        indexed_png(32, 16, png::BitDepth::Eight, &vec![vec![0; 32]; 16]),
    )
    .unwrap();
    let err = SpriteSheet::load(&dir.join("walker.json")).unwrap_err();
    assert!(
        err.message
            .contains("variant `night` is 32x16, but the sheet image is 64x16"),
        "{err}"
    );
    std::fs::remove_file(dir.join("night.png")).unwrap();
    let err = SpriteSheet::load(&dir.join("walker.json")).unwrap_err();
    assert!(err.path.ends_with("night.png"), "{err}");
}

#[test]
fn atlas_from_sheet_frames() {
    let s = walker();
    let mut image = Pixels::new(64, 16);
    for (i, f) in s.frames.iter().enumerate() {
        image.set(f.rect.x, f.rect.y, i as u8 + 1);
    }
    let mut builder = AtlasBuilder::default();
    let ids: Vec<usize> = s
        .frames
        .iter()
        .map(|f| builder.add(&image, f.rect))
        .collect();
    let atlas = builder.build(32, 1).unwrap();
    // one 16px frame per 32px page, but the trimmed 12px one fits beside it
    assert_eq!(atlas.pages.len(), 3);
    for (i, &id) in ids.iter().enumerate() {
        let e = atlas.entries[id];
        assert_eq!(
            (e.rect.w, e.rect.h),
            (s.frames[i].rect.w, s.frames[i].rect.h)
        );
        assert_eq!(atlas.pages[e.page].get(e.rect.x, e.rect.y), i as u8 + 1);
    }
}
