use std::path::Path;

use clamber_level::{
    Area, Depth, Dir, Fixed, LayerContent, Level, Lighting, Rect, Shape, Tile, Vec2, tiled,
};
use clamber_pack::{Adjust, Param};

fn f(x: f64) -> Fixed {
    Fixed::from_f64(x).unwrap()
}

fn v(x: f64, y: f64) -> Vec2 {
    Vec2::new(f(x), f(y))
}

fn full() -> Level {
    Level::from_toml_str(include_str!("fixtures/full.level"), "full.level").unwrap()
}

/// The error for a minimal level with `extra` appended.
fn level_err(extra: &str) -> String {
    let src = format!("format = 1\nsize = [100, 100]\nstart = [10, 10]\n{extra}");
    Level::from_toml_str(&src, "x.level")
        .unwrap_err()
        .to_string()
}

#[test]
fn reads_every_part() {
    let l = full();
    assert_eq!(l.name, "Everything");
    assert_eq!(l.size, v(1600.0, 600.0));

    let names: Vec<_> = l.layers.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["sky", "ground tiles", "fog"]);
    assert_eq!(l.layers[0].parallax, v(0.25, 0.5));
    assert_eq!(l.layers[2].depth, Depth::Front);
    assert_eq!(l.layers[2].offset, v(0.0, -32.0));
    assert_eq!((l.layers[0].scale, l.layers[0].pixel_art), (1, false));
    assert_eq!((l.layers[1].scale, l.layers[1].pixel_art), (4, true));
    let LayerContent::Tiles(tiles) = &l.layers[1].content else {
        panic!("expected tiles");
    };
    assert_eq!(tiles.columns, 4);
    assert_eq!(tiles.tiles[0].index(), None);
    assert_eq!(tiles.tiles[1].index(), Some(0));
    let flipped = tiles.tiles[4];
    assert_eq!(flipped.index(), Some(4));
    assert_ne!(flipped.0 & Tile::FLIP_X, 0);

    assert_eq!(l.entities[0].kind, "spring");
    assert_eq!(l.entities[0].id.as_deref(), Some("spring-a"));
    assert_eq!(l.entities[0].params["strength"].as_float(), Some(10.5));
    assert_eq!(l.entities[1].at, v(400.5, 450.0));

    // the polyline becomes two segments
    assert_eq!(l.collision.colliders.len(), 6);
    assert!(matches!(
        l.collision.colliders[2].shape,
        Shape::Segment { one_way: true, .. }
    ));
    assert_eq!(l.collision.colliders[4].surface, "hazard");
    assert!(matches!(
        l.collision.colliders[5].shape,
        Shape::Solid { one_way: true, .. }
    ));

    let LayerContent::Image { normals, .. } = &l.layers[0].content else {
        panic!("expected an image");
    };
    assert_eq!(normals.as_deref(), Some("bg/sky-normals.png"));
    assert_eq!(
        tiles.tileset.normals.as_deref(),
        Some("tiles/ground-normals.png")
    );
    let LayerContent::Image { normals, .. } = &l.layers[2].content else {
        panic!("expected an image");
    };
    assert_eq!(*normals, None);

    assert_eq!(l.lighting.ambient, [0x40, 0x48, 0x58]);
    assert_eq!(l.lighting.key.direction, [1.0, -2.0, 1.0]);
    assert_eq!(l.lighting.key.intensity, 0.75);
    assert_eq!(l.lighting.key.color, Lighting::default().key.color);
    assert_eq!(l.lights.len(), 2);
    assert_eq!(l.lights[0].at, v(320.0, 470.0));
    assert_eq!(l.lights[0].color, [0xff, 0xc0, 0x70]);
    assert_eq!((l.lights[0].radius, l.lights[0].intensity), (f(96.0), 1.5));
    assert_eq!(l.lights[1].color, [0xff; 3]);
    assert_eq!(l.lights[1].intensity, 1.0);

    let music = l.music.unwrap();
    assert_eq!(music.file, "music/everything.ogg");
    assert_eq!(music.loop_start, Some(44100));
    assert_eq!(music.loop_length, Some(88200));
}

#[test]
fn surfaces_built_in_and_overridden() {
    let l = full();
    assert!(
        l.surfaces["slippery"]
            .physics
            .0
            .contains_key(&Param::GroundFriction)
    );
    assert!(
        l.surfaces["ice"]
            .physics
            .0
            .contains_key(&Param::GroundFriction)
    );
    let hazard = &l.surfaces["hazard"];
    assert!(hazard.hazard);
    assert_eq!(hazard.physics.0[&Param::TopSpeed], Adjust::Set(f(1.0)));

    let hit = l
        .collision
        .cast(v(700.0, 0.0), Dir::Down, f(600.0))
        .unwrap();
    assert_eq!(l.surface(hit.collider).physics, l.surfaces["ice"].physics);
    assert_eq!(hit.point, v(700.0, 450.0));
}

#[test]
fn regions_and_camera() {
    let l = full();
    let names = |p| -> Vec<_> { l.regions_at(p).map(|r| r.name.clone()).collect() };
    assert_eq!(names(v(1100.0, 450.0)), [Some("pool".to_string())]);
    assert_eq!(names(v(300.0, 100.0)), [None]);
    assert!(names(v(100.0, 250.0)).is_empty());

    let pool = &l.regions[0];
    assert!(pool.water);
    let physics = pool.physics.as_ref().unwrap();
    assert_eq!(physics.0[&Param::Gravity], Adjust::Scale(f(0.5)));
    assert_eq!(l.regions[1].darkness, Some(f(0.75)));
    assert_eq!(l.regions[1].wind, Some(v(-0.125, 0.0)));

    let open = l.camera_at(v(100.0, 450.0));
    assert_eq!(open.look_ahead, Some(v(48.0, 0.0)));
    assert_eq!(open.limits.unwrap().w, f(1600.0));
    assert_eq!(open.lock_x, None);
    let shaft = l.camera_at(v(1450.0, 300.0));
    assert_eq!(shaft.lock_x, Some(f(1500.0)));
    assert_eq!(shaft.limits.unwrap().x, f(1400.0));
    assert_eq!(shaft.look_ahead, Some(v(48.0, 0.0)), "inherited");
}

#[test]
fn writes_and_reads_back_the_same() {
    let l = full();
    let text = l.to_toml_string();
    assert!(text.starts_with("# clamber level v1\n"));
    let back = Level::from_toml_str(&text, "saved").unwrap();
    assert_eq!(back, l);
}

#[test]
fn scenario_test_levels_fit_their_scenarios() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/test");

    // run-jump-flat: flat ground with at least 320 px clear ahead
    let l = Level::load(&dir.join("run-jump-flat.level")).unwrap();
    let ground = l.collision.cast(l.start, Dir::Down, f(1.0)).unwrap();
    assert_eq!(ground.distance, Fixed::ZERO);
    let above_ground = Vec2::new(l.start.x, l.start.y - Fixed::ONE);
    assert_eq!(l.collision.cast(above_ground, Dir::Right, f(320.0)), None);
    for x in [64.0, 384.0] {
        let hit = l.collision.cast(v(x, 0.0), Dir::Down, f(1000.0)).unwrap();
        assert_eq!(hit.point.y, l.start.y, "flat at x = {x}");
    }
    assert!(l.entities.is_empty());

    // walk-off-ledge: a ledge 48 to 64 px ahead, then a drop of at least 96
    let l = Level::load(&dir.join("walk-off-ledge.level")).unwrap();
    let mut x = l.start.x;
    while l
        .collision
        .cast(Vec2::new(x, l.start.y), Dir::Down, f(1.0))
        .is_some()
    {
        x += Fixed::ONE;
    }
    let ahead = x - l.start.x;
    assert!(
        ahead >= f(48.0) && ahead <= f(64.0),
        "ledge {ahead} px ahead"
    );
    let drop = l
        .collision
        .cast(Vec2::new(x, l.start.y), Dir::Down, f(1000.0))
        .unwrap();
    assert!(drop.distance >= f(96.0), "drop of {}", drop.distance);
}

#[test]
fn format_errors() {
    let wrong = Level::from_toml_str("format = 2\nsize = [1, 1]\nstart = [0, 0]", "x.level");
    assert_eq!(
        wrong.unwrap_err().to_string(),
        "x.level: format: version 2 isn't supported; this engine reads 1"
    );
    assert!(level_err("[[collision]]\nshape = \"circle\"\n").contains("circle"));
    assert!(
        level_err("[[layers]]\nname = \"sky\"\nimage = \"sky.png\"\nscale = 0\n")
            .contains("layers[0].scale: must be at least 1")
    );
    assert!(
        level_err("[[collision]]\nshape = \"rect\"\nrect = [0, 0, 1, 1]\nsurface = \"lava\"\n")
            .contains("collision[0].surface: unknown surface `lava`")
    );
    assert!(
        level_err(
            "[[collision]]\nshape = \"polygon\"\npoints = [[0, 0], [10, 10], [10, 0], [0, 10]]\n"
        )
        .contains("collision[0].points: polygon crosses itself")
    );
    assert!(
        level_err("[[collision]]\nshape = \"segment\"\nfrom = [1, 1]\nto = [1, 1]\n")
            .contains("segment has zero length")
    );
    assert!(
        level_err("[[collision]]\nshape = \"rect\"\nrect = [0, 0, 1, 1]\nwidth = 3\n")
            .contains("width")
    );
    assert!(level_err("[[regions]]\nwater = true\n").contains("give either `rect` or `points`"));
    assert!(
        level_err("[[regions]]\nrect = [0, 0, 5, 5]\ndarkness = 2\n")
            .contains("regions[0].darkness: must be between 0 and 1")
    );
    assert!(
        level_err("[[regions]]\nrect = [0, 0, 5, 5]\nphysics = { gravty = 1 }\n")
            .contains("regions[0].physics.gravty: unknown movement parameter")
    );
    assert!(level_err("[[layers]]\nname = \"a\"\n").contains("give either `image` or `tiles`"));
    assert!(
        level_err("[[layers]]\nname = \"a\"\nimage = \"a.png\"\n[[layers]]\nname = \"a\"\nimage = \"b.png\"\n")
            .contains("a layer named `a` already exists")
    );
    assert!(
        level_err("[[layers]]\nname = \"t\"\ntiles = { tileset = \"t.png\", tile_size = [8, 8], columns = 3, data = [0, 0, 0, 0] }\n")
            .contains("4 tiles don't fill rows of 3 columns")
    );
    assert!(
        level_err("[[entities]]\ntype = \"a\"\nid = \"x\"\nat = [0, 0]\n[[entities]]\ntype = \"b\"\nid = \"x\"\nat = [0, 0]\n")
            .contains("entities[1].id: `x` is used twice")
    );
    assert!(
        level_err("[[entities]]\ntype = \"Big Thing\"\nat = [0, 0]\n").contains("entities[0].type")
    );
    assert!(level_err("[[entities]]\ntype = \"a\"\nat = [5e9, 0]\n").contains("out of range"));
    assert!(
        level_err("[music]\nfile = \"a.ogg\"\nloop_length = 10\n")
            .contains("music.loop_length: needs `loop_start`")
    );
    assert!(level_err("[music]\nfile = \"\"\n").contains("music.file: is empty"));
    assert!(
        level_err("[lighting]\nambient = \"grey\"\n")
            .contains("lighting.ambient: `grey` isn't a `#rrggbb` colour")
    );
    assert!(
        level_err("[lighting]\nkey = { direction = [0, 0, 0] }\n")
            .contains("lighting.key.direction: can't be all zero")
    );
    assert!(
        level_err("[[lights]]\nat = [5, 5]\nradius = 0\n")
            .contains("lights[0].radius: must be positive")
    );
    assert!(
        level_err("[[lights]]\nat = [5, 5]\nradius = 9\nintensity = -1\n")
            .contains("lights[0].intensity: can't be negative")
    );
    assert!(
        level_err("[[layers]]\nname = \"t\"\nnormals = \"n.png\"\ntiles = { tileset = \"t.png\", tile_size = [8, 8], columns = 1, data = [0] }\n")
            .contains("layers[0].normals: is for image layers")
    );
    let outside = Level::from_toml_str("format = 1\nsize = [10, 10]\nstart = [10, 0]", "x.level");
    assert!(
        outside
            .unwrap_err()
            .to_string()
            .contains("start: is outside the level")
    );
}

#[test]
fn imports_tiled_flips_in_clamber_order() {
    // Tiled swaps x and y before mirroring; Clamber mirrors first.
    let (h, v, d) = (0x8000_0000u32, 0x4000_0000u32, 0x2000_0000u32);
    let data = [d | 1, d | h | 1, d | v | 1, d | h | v | 1, h | 1, v | 1];
    let map = format!(
        r#"{{ "type": "map", "orientation": "orthogonal", "infinite": false,
            "width": 6, "height": 1, "tilewidth": 8, "tileheight": 8,
            "tilesets": [ {{ "firstgid": 1, "name": "t", "image": "t.png",
                "imagewidth": 8, "imageheight": 8, "tilewidth": 8, "tileheight": 8,
                "tilecount": 1, "columns": 1, "margin": 0, "spacing": 0 }} ],
            "layers": [ {{ "type": "tilelayer", "id": 1, "name": "t", "x": 0, "y": 0,
                "width": 6, "height": 1, "data": {data:?} }},
                {{ "type": "objectgroup", "id": 2, "name": "things", "objects": [
                    {{ "id": 1, "name": "", "class": "start", "point": true, "x": 4, "y": 4,
                       "width": 0, "height": 0, "rotation": 0 }} ] }} ] }}"#
    );
    let l = tiled::import(&map, "flips.json", |_| Err("none".into())).unwrap();
    let LayerContent::Tiles(t) = &l.layers[0].content else {
        panic!("expected tiles");
    };
    let flags: Vec<u32> = t.tiles.iter().map(|t| t.0 & !1).collect();
    let (x, y, diagonal) = (Tile::FLIP_X, Tile::FLIP_Y, Tile::FLIP_DIAGONAL);
    assert_eq!(
        flags,
        [diagonal, diagonal | y, diagonal | x, diagonal | x | y, x, y]
    );
}

#[test]
fn imports_tiled_map() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiled/map.json");
    let l = tiled::import_file(&path).unwrap();
    assert_eq!(l.name, "From Tiled");
    assert_eq!(l.size, v(64.0, 48.0));
    assert_eq!(l.start, v(8.0, 32.0));
    assert_eq!(l.camera.rule.look_ahead, Some(v(40.0, 0.0)));
    assert_eq!(l.music.as_ref().unwrap().file, "theme.ogg");

    // layers: the image, then the tile layer split per tileset
    let names: Vec<_> = l.layers.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["sky", "ground", "ground 2"]);
    assert_eq!(l.layers[0].parallax, v(0.5, 0.25));
    let LayerContent::Image { repeat_x, .. } = l.layers[0].content else {
        panic!("expected an image layer");
    };
    assert!(repeat_x);
    // group offset and parallax combine with the layer's own
    assert_eq!(l.layers[1].offset, v(8.0, 4.0));
    assert_eq!(l.layers[1].parallax, v(0.5, 1.0));
    let tiles = |i: usize| match &l.layers[i].content {
        LayerContent::Tiles(t) => t.clone(),
        _ => panic!("expected tiles"),
    };
    let ground = tiles(1);
    assert_eq!(ground.tileset.image, "ground.png");
    let ids: Vec<u32> = ground.tiles.iter().map(|t| t.0).collect();
    assert_eq!(ids, [0, 0, 0, 0, 0, 0, 0, 0, 1, 2, Tile::FLIP_X | 3, 4]);
    let plants = tiles(2);
    assert_eq!(plants.tileset.image, "deco/plants.png");
    assert_eq!((plants.tileset.margin, plants.tileset.spacing), (1, 2));
    assert_eq!(plants.tiles[5].index(), Some(0));
    assert_eq!(plants.tiles[6].index(), Some(1));

    // collision: an untyped rectangle on the collision layer, a platform's
    // top edge, and a polyline
    let c = &l.collision.colliders;
    assert_eq!(c.len(), 3);
    assert!(matches!(&c[0].shape, Shape::Solid { points, one_way: false } if points.len() == 4));
    assert_eq!(
        c[1].shape,
        Shape::Segment {
            a: v(16.0, 8.0),
            b: v(48.0, 8.0),
            one_way: true
        }
    );
    assert_eq!(c[2].surface, "slippery");

    let door = &l.entities[0];
    assert_eq!(
        (door.kind.as_str(), door.id.as_deref()),
        ("door", Some("door-1"))
    );
    assert_eq!(door.at, v(40.0, 16.0));
    assert_eq!(door.params["target"].as_str(), Some("level-2"));
    assert_eq!(door.params["width"].as_float(), Some(16.0));

    let r = &l.regions[0];
    assert!(r.water);
    assert_eq!(r.wind, Some(v(0.25, 0.0)));
    let physics = r.physics.as_ref().unwrap();
    assert_eq!(physics.0[&Param::Gravity], Adjust::Scale(f(0.5)));
    assert_eq!(physics.0[&Param::MaxFallSpeed], Adjust::Set(f(2.0)));
    let limits = r.camera.unwrap().limits.unwrap();
    assert_eq!(
        limits,
        Rect {
            x: f(0.0),
            y: f(0.0),
            w: f(32.0),
            h: f(48.0)
        }
    );
    assert!(matches!(r.area, Area::Rect(_)));

    // and it saves as a normal level
    let back = Level::from_toml_str(&l.to_toml_string(), "saved").unwrap();
    assert_eq!(back, l);
}

#[test]
fn tiled_errors() {
    let map = |extra: &str, layers: &str| {
        format!(
            r#"{{ "orientation": "orthogonal", "width": 2, "height": 2, "tilewidth": 8, "tileheight": 8,
                 {extra} "layers": [{layers}] }}"#
        )
    };
    let start = r#"{ "type": "objectgroup", "name": "o", "objects": [
        { "id": 1, "type": "start", "point": true, "x": 1, "y": 1 } ] }"#;
    let import = |src: String| {
        tiled::import(&src, "m.json", |_| Err("no files".into()))
            .unwrap_err()
            .to_string()
    };
    assert!(import(map("", "")).contains("no `start` object"));
    assert!(import(map(r#""infinite": true,"#, start)).contains("infinite maps"));
    assert!(
        import(map(
            r#""tilesets": [{ "firstgid": 1, "source": "t.tsj" }],"#,
            start
        ))
        .contains("tileset t.tsj: no files")
    );
    assert!(import(map(
        "",
        &format!(
            r#"{start}, {{ "type": "tilelayer", "name": "t", "width": 2, "encoding": "base64", "data": "AAAA" }}"#
        )
    ))
    .contains("set Tile Layer Format to CSV"));
    assert!(
        import(map(
            "",
            r#"{ "type": "objectgroup", "name": "o", "objects": [
            { "id": 7, "type": "", "x": 1, "y": 1, "width": 2, "height": 2 } ] }"#
        ))
        .contains("object 7: has no class")
    );
    assert!(
        import(map(
            "",
            &format!(
                r#"{start}, {{ "type": "objectgroup", "name": "o", "objects": [
            {{ "id": 8, "type": "region", "x": 1, "y": 1, "width": 2, "height": 2,
               "properties": [ {{ "name": "colour", "type": "string", "value": "red" }} ] }} ] }}"#
            )
        ))
        .contains("object 8: unknown region property `colour`")
    );
}
