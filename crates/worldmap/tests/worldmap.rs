use clamber_level::{Fixed, Vec2};
use clamber_worldmap::{
    Condition, Mode, NodeKind, Position, Progress, Walker, WorldMap, check_warps,
};

fn overworld() -> WorldMap {
    WorldMap::from_toml_str(include_str!("fixtures/overworld.map"), "overworld.map").unwrap()
}

fn v(x: i32, y: i32) -> Vec2 {
    Vec2::ints(x, y)
}

/// The error for a minimal path map with `extra` appended.
fn map_err(extra: &str) -> String {
    let src = format!(
        "format = 1\nid = \"m\"\nsize = [100, 100]\nstart = \"a\"\n\
         [[nodes]]\nid = \"a\"\nkind = \"waypoint\"\nat = [0, 0]\n\
         [[nodes]]\nid = \"lv\"\nkind = \"level\"\nlevel = \"x.level\"\nat = [50, 0]\n{extra}"
    );
    WorldMap::from_toml_str(&src, "m.map")
        .unwrap_err()
        .to_string()
}

fn ids(map: &WorldMap, nodes: impl IntoIterator<Item = usize>) -> Vec<&str> {
    nodes
        .into_iter()
        .map(|i| map.nodes[i].id.as_str())
        .collect()
}

#[test]
fn reads_nodes_paths_and_decorations() {
    let m = overworld();
    assert_eq!(m.id, "overworld");
    assert_eq!(m.mode, Mode::Paths);
    assert_eq!(m.start.as_deref(), Some("home"));
    assert_eq!(m.nodes.len(), 6);
    assert_eq!(
        m.nodes[1].kind,
        NodeKind::Level {
            level: "levels/meadow.level".into(),
            exits: vec!["normal".into(), "secret".into()]
        }
    );
    assert!(
        matches!(&m.nodes[3].kind, NodeKind::Event { event, params } if event == "shop" && params["stock"].as_str() == Some("basic"))
    );
    assert_eq!(
        m.nodes[4].kind,
        NodeKind::Warp {
            map: "forest".into(),
            node: "entry".into()
        }
    );
    // unqualified conditions are qualified with the map
    assert_eq!(
        m.nodes[5].unlock,
        Some(Condition::Cleared {
            place: "overworld:meadow".into(),
            by: Some("secret".into())
        })
    );
    assert_eq!(m.decorations[0].animation.as_deref(), Some("roll"));
}

#[test]
fn progress_unlocks_the_way() {
    let m = overworld();
    let mut p = Progress::default();
    let home = m.node("home").unwrap();
    let meadow = m.node("meadow").unwrap();

    let exits: Vec<_> = m.exits(meadow, &p).iter().map(|e| e.to).collect();
    assert_eq!(ids(&m, exits), ["home"], "the way on is locked");

    p.clear("overworld:meadow", "normal");
    let exits: Vec<_> = m.exits(meadow, &p).iter().map(|e| e.to).collect();
    assert_eq!(ids(&m, exits), ["home", "fork"]);

    let fork = m.node("fork").unwrap();
    let gate = m.node("forest-gate").unwrap();
    assert!(!m.node_unlocked(gate, &p));
    let exits: Vec<_> = m.exits(fork, &p).iter().map(|e| e.to).collect();
    assert_eq!(ids(&m, exits), ["meadow", "shop"]);
    p.add("stars", 3);
    assert!(m.node_unlocked(gate, &p));
    assert_eq!(m.exits(fork, &p).len(), 3);

    // walking a path back reverses it
    let back = m
        .exits(meadow, &p)
        .into_iter()
        .find(|e| e.to == home)
        .unwrap();
    assert_eq!(back.line[0], v(200, 400));
    assert_eq!(*back.line.last().unwrap(), v(64, 400));
}

#[test]
fn hidden_path_revealed_once() {
    let m = overworld();
    let mut p = Progress::default();
    let grotto = m.node("grotto").unwrap();
    assert!(!m.node_visible(grotto, &p));
    p.clear("overworld:meadow", "normal");
    assert!(
        m.reveal(&mut p).is_empty(),
        "the normal exit doesn't reveal it"
    );

    p.clear("overworld:meadow", "secret");
    assert!(m.node_visible(grotto, &p));
    assert_eq!(
        m.reveal(&mut p),
        ["overworld:grotto", "overworld:meadow>grotto"]
    );
    assert!(m.reveal(&mut p).is_empty(), "reported only once");

    // the hidden path is one-way: no way back from the grotto
    let exits = m.exits(grotto, &p);
    assert!(exits.is_empty());
}

#[test]
fn direction_picks_the_path() {
    let m = overworld();
    let mut p = Progress::default();
    p.clear("overworld:meadow", "normal");
    p.add("stars", 5);
    let fork = m.node("fork").unwrap();
    let pick = |dir| {
        m.exit_towards(fork, dir, &p)
            .map(|e| m.nodes[e.to].id.clone())
    };
    assert_eq!(pick(v(0, -1)).as_deref(), Some("shop"));
    assert_eq!(pick(v(1, 0)).as_deref(), Some("forest-gate"));
    assert_eq!(pick(v(-1, 1)).as_deref(), Some("meadow"));
    let meadow = m.node("meadow").unwrap();
    // nothing leaves the meadow downwards
    assert_eq!(m.exit_towards(meadow, v(0, 1), &p), None);
}

#[test]
fn walking_a_curved_path() {
    let m = overworld();
    let line = m.path_line(1);
    assert_eq!(line[0], v(200, 400));
    assert_eq!(*line.last().unwrap(), v(300, 300));
    // the curve passes through its waypoint
    assert!(line.contains(&v(260, 380)));

    let mut w = Walker::new(line.clone());
    let total = clamber_worldmap::length(&line);
    let mut steps = 0;
    while !w.arrived() {
        w.advance(Fixed::from_int(2));
        steps += 1;
    }
    assert_eq!(w.position(), v(300, 300));
    let expected =
        (total.to_bits() + Fixed::from_int(2).to_bits() - 1) / Fixed::from_int(2).to_bits();
    assert_eq!(steps, expected);
}

#[test]
fn free_roaming_map() {
    let m = WorldMap::from_toml_str(include_str!("fixtures/island.map"), "island.map").unwrap();
    let Mode::Free(free) = &m.mode else {
        panic!("expected a free-roaming map");
    };
    assert_eq!(free.start, v(400, 400));
    assert_eq!(free.speed, Fixed::from_f64(1.5).unwrap());
    assert_eq!(free.vehicle.as_ref().unwrap().speed, Fixed::from_int(3));

    let mut p = Progress::default();
    assert_eq!(m.node_touching(v(410, 690), &p), m.node("dock"));
    assert_eq!(m.node_touching(v(400, 650), &p), None, "outside the radius");
    assert_eq!(m.node_touching(v(105, 100), &p), None, "locked");
    p.flags.insert("map-found".into());
    assert_eq!(m.node_touching(v(105, 100), &p), m.node("cave"));
}

#[test]
fn progress_round_trips_as_save_data() {
    let mut p = Progress::default();
    p.clear("overworld:meadow", "secret");
    p.flags.insert("bridge".into());
    p.add("stars", 4);
    p.revealed.insert("overworld:grotto".into());
    p.position = Some(Position {
        map: "island".into(),
        node: None,
        at: Some([
            Fixed::from_int(400).to_bits(),
            Fixed::from_f64(0.5).unwrap().to_bits(),
        ]),
    });
    let text = toml::to_string(&p).unwrap();
    let back: Progress = toml::from_str(&text).unwrap();
    assert_eq!(back, p);
}

#[test]
fn conditions_on_other_maps_are_taken_as_written() {
    let src = "format = 1\nid = \"m\"\nsize = [100, 100]\nstart = \"a\"\n\
               [[nodes]]\nid = \"a\"\nkind = \"save\"\nat = [0, 0]\n\
               unlock = { cleared = \"forest:boss\", by = \"secret\" }\n";
    let m = WorldMap::from_toml_str(src, "m.map").unwrap();
    let mut p = Progress::default();
    assert!(!m.node_unlocked(0, &p));
    p.clear("forest:boss", "secret");
    assert!(m.node_unlocked(0, &p));
}

#[test]
fn format_errors() {
    assert!(
        map_err("[[nodes]]\nid = \"a\"\nkind = \"save\"\nat = [1, 1]\n")
            .contains("nodes[2].id: `a` is used twice")
    );
    assert!(
        map_err("[[nodes]]\nid = \"b\"\nkind = \"castle\"\nat = [1, 1]\n")
            .contains("use level, warp, save, event or waypoint")
    );
    assert!(
        map_err("[[nodes]]\nid = \"b\"\nkind = \"warp\"\nmap = \"x\"\nat = [1, 1]\n")
            .contains("warp nodes need `node`")
    );
    assert!(
        map_err("[[nodes]]\nid = \"b\"\nkind = \"save\"\nlevel = \"x\"\nat = [1, 1]\n")
            .contains("nodes[2].level: not used by save nodes")
    );
    assert!(
        map_err("[[paths]]\nfrom = \"a\"\nto = \"zz\"\n").contains("paths[0].to: no node `zz`")
    );
    assert!(
        map_err("[[paths]]\nfrom = \"a\"\nto = \"lv\"\n[[paths]]\nfrom = \"lv\"\nto = \"a\"\n")
            .contains("already joined")
    );
    assert!(
        map_err("[[paths]]\nfrom = \"a\"\nto = \"lv\"\nunlock = { cleared = \"a\" }\n")
            .contains("`a`: isn't a level node")
    );
    assert!(
        map_err(
            "[[paths]]\nfrom = \"a\"\nto = \"lv\"\nunlock = { cleared = \"lv\", by = \"secret\" }\n"
        )
        .contains("`lv` has no exit `secret`")
    );
    assert!(
        map_err(
            "[[paths]]\nfrom = \"a\"\nto = \"lv\"\nunlock = { flag = \"x\", counter = \"y\" }\n"
        )
        .contains("use one of")
    );
    assert!(
        map_err("[[paths]]\nfrom = \"a\"\nto = \"lv\"\nunlock = { any = [{ flag = 3 }] }\n")
            .contains("paths[0].unlock.any[0].flag: expected a string")
    );
    assert!(
        map_err("[free]\nstart = [0, 0]\nspeed = 1\n")
            .contains("only for maps with mode = \"free\"")
    );
}

#[test]
fn warps_checked_across_maps() {
    let island =
        WorldMap::from_toml_str(include_str!("fixtures/island.map"), "island.map").unwrap();
    let maps = [overworld(), island];
    // the island's dock leads home; the forest map isn't in the set
    assert_eq!(
        check_warps(&maps),
        ["overworld:forest-gate warps to unknown map `forest`"]
    );
    let doubled = [overworld(), overworld()];
    assert!(check_warps(&doubled).contains(&"map id `overworld` is used twice".to_string()));
}

#[test]
fn reads_the_maps_music() {
    assert_eq!(overworld().music, None);
    let src = "format = 1\nid = \"m\"\nsize = [100, 100]\nstart = \"a\"\n\
         [[nodes]]\nid = \"a\"\nkind = \"waypoint\"\nat = [0, 0]\n\
         [music]\nfile = \"m.ogg\"\nloop_start = 10\nloop_length = 20\n";
    let music = WorldMap::from_toml_str(src, "m.map")
        .unwrap()
        .music
        .unwrap();
    assert_eq!(music.file, "m.ogg");
    assert_eq!((music.loop_start, music.loop_length), (Some(10), Some(20)));
    let err = map_err("[music]\nfile = \"\"\n");
    assert!(err.contains("music.file"), "{err}");
    let err = map_err("[music]\nfile = \"m.ogg\"\nloop_length = 5\n");
    assert!(err.contains("loop_start"), "{err}");
}
