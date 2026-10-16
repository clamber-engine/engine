use clamber_pack::{FeelWarning, Fixed, Pack, Param, Presets, read_physics_patch};

fn fx(x: f64) -> Fixed {
    Fixed::from_f64(x).unwrap()
}

fn example() -> Pack {
    let src = include_str!("fixtures/example.toml");
    Pack::from_toml_str(src, "example.toml").unwrap()
}

fn load(src: &str) -> String {
    Pack::from_toml_str(src, "pack.toml")
        .unwrap_err()
        .to_string()
}

#[test]
fn loads_roster_in_order() {
    let pack = example();
    let ids: Vec<_> = pack.roster.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["runner", "glider", "climber"]);
    assert_eq!(pack.preset, "example-feel");
    assert_eq!(pack.roster[1].abilities["glide"]["turn_speed"], fx(0.125));
    assert!(pack.roster[0].abilities["roll"].is_empty());

    let presets: Vec<_> = pack.presets().iter().map(|p| p.id.as_str()).collect();
    assert_eq!(presets.last(), Some(&"example-feel"));
}

#[test]
fn designed_preset_layers_world_profile_and_slot() {
    let selection = example().select_feel(None).unwrap();
    assert_eq!(selection.warnings, []);
    let feel = selection.feel;
    let snappy = Presets::builtin().world("snappy").unwrap();

    // world: default, then snappy, then the pack's preset
    assert_eq!(feel.world[Param::Gravity], snappy[Param::Gravity]);
    assert_eq!(feel.world[Param::MaxFallSpeed], fx(14.0));

    // slot 0 has no profile or slot patch
    assert_eq!(feel.roster[0].physics, feel.world);

    // slot 1: profile scales top speed, then the preset sets it outright
    let glider = &feel.roster[1];
    assert_eq!(glider.physics[Param::TopSpeed], fx(4.0));
    assert_eq!(glider.physics[Param::JumpSpeed], fx(5.5));
    assert_eq!(glider.abilities["glide"]["fall_speed"], fx(0.75));
    assert_eq!(glider.abilities["glide"]["turn_speed"], fx(0.125));

    let climber = &feel.roster[2];
    assert_eq!(climber.physics[Param::GroundAcceleration], fx(0.09375));
    assert_eq!(
        climber.physics[Param::TopSpeed],
        feel.world[Param::TopSpeed]
    );
}

#[test]
fn other_presets_keep_character_profiles_and_warn() {
    let pack = example();
    let selection = pack.select_feel(Some("floaty")).unwrap();
    assert_eq!(
        selection.warnings,
        [FeelWarning::NotDesignedFor {
            designed_for: "example-feel".into(),
            chosen: "floaty".into(),
        }]
    );
    let floaty = Presets::builtin().world("floaty").unwrap();
    let feel = selection.feel;
    assert_eq!(feel.world, floaty);
    // The pack's own preset doesn't apply, but the glider's profile does.
    let glider = &feel.roster[1].physics;
    assert_eq!(glider[Param::TopSpeed], floaty[Param::TopSpeed] * fx(0.875));
    assert_eq!(feel.roster[1].abilities["glide"]["fall_speed"], fx(1.0));
}

#[test]
fn placeholder_presets_warn() {
    let selection = example().select_feel(Some("classic-2")).unwrap();
    assert_eq!(selection.warnings.len(), 2);
    assert_eq!(
        selection.warnings[1],
        FeelWarning::Placeholder {
            preset: "classic-2".into()
        }
    );
    assert_eq!(
        selection.warnings[1].to_string(),
        "the `classic-2` preset is a placeholder and its values may change"
    );
    // Only some of it is filled in so far, such as its jump timing,
    // gravity, speeds, turns, rolls, slopes and knock-out; the rest,
    // such as swapping characters, is Default's.
    let default = Presets::builtin().world("default").unwrap();
    assert_eq!(
        selection.feel.world[Param::SwapMoves],
        default[Param::SwapMoves]
    );
    assert_ne!(
        selection.feel.world[Param::Gravity],
        default[Param::Gravity]
    );
    assert_ne!(
        selection.feel.world[Param::TopSpeed],
        default[Param::TopSpeed]
    );
    assert_ne!(
        selection.feel.world[Param::TakeoffWait],
        default[Param::TakeoffWait]
    );
}

#[test]
fn unknown_choice_is_an_error() {
    let err = example().select_feel(Some("nope")).unwrap_err();
    assert_eq!(err.to_string(), "no preset named `nope`");
}

#[test]
fn minimal_pack_uses_default() {
    let pack = Pack::from_toml_str(
        "id = \"p\"\nname = \"P\"\n[[roster]]\nid = \"a\"\nname = \"A\"\n",
        "p",
    )
    .unwrap();
    assert_eq!(pack.preset, "default");
    let selection = pack.select_feel(None).unwrap();
    assert_eq!(selection.warnings, []);
    assert_eq!(
        selection.feel.world,
        Presets::builtin().world("default").unwrap()
    );
}

#[test]
fn region_overrides_adjust_resolved_physics() {
    let feel = example().select_feel(None).unwrap().feel;
    let table: toml::Table = toml::from_str("gravity = { scale = 0.5 }\ntop_speed = 2").unwrap();
    let water = read_physics_patch(&table, "level", "regions.pool.physics").unwrap();
    let wet = feel.roster[2].physics.patched(&water);
    assert_eq!(
        wet[Param::Gravity] * fx(2.0),
        feel.roster[2].physics[Param::Gravity]
    );
    assert_eq!(wet[Param::TopSpeed], fx(2.0));
    assert_eq!(wet[Param::GroundAcceleration], fx(0.09375));

    let bad: toml::Table = toml::from_str("swim = 1").unwrap();
    let err = read_physics_patch(&bad, "level", "regions.pool.physics").unwrap_err();
    assert_eq!(
        err.to_string(),
        "level: regions.pool.physics.swim: unknown movement parameter"
    );
}

#[test]
fn rejects_bad_packs() {
    let head = "id = \"p\"\nname = \"P\"\n";
    let one = "[[roster]]\nid = \"a\"\nname = \"A\"\n";
    let cases = [
        (format!("{head}roster = []\n"), "pack.toml: roster: a pack needs at least one character".to_string()),
        (
            format!("{head}{one}{one}"),
            "pack.toml: roster[1].id: `a` is already in the roster".into(),
        ),
        (
            format!("{head}preset = \"nope\"\n{one}"),
            "pack.toml: preset: no preset named `nope`".into(),
        ),
        (
            format!("{head}{one}profile.top_speed = -1\n"),
            "pack.toml: roster[0].profile.top_speed: must not be negative".into(),
        ),
        (
            format!("{head}{one}abilities.glide = {{ speed = {{ scale = 2 }} }}\n"),
            "pack.toml: roster[0].abilities.glide.speed: expected a number".into(),
        ),
        (
            format!("{head}{one}abilities.Glide = {{}}\n"),
            "pack.toml: roster[0].abilities.Glide: `Glide` is not a valid name (use a-z, 0-9 and _)".into(),
        ),
        (
            format!("{head}{one}[presets.snappy]\nlabel = \"Mine\"\n"),
            "pack.toml: presets.snappy: a built-in preset already has this id".into(),
        ),
        (
            format!("{head}{one}[presets.mine]\nlabel = \"Mine\"\nbase = \"theirs\"\n"),
            "pack.toml: presets.mine.base: no preset named `theirs`".into(),
        ),
    ];
    for (src, want) in cases {
        assert_eq!(load(&src), want, "{src}");
    }

    // Structural errors come from the TOML parser, with a location.
    let err = load(&format!("{head}{one}speed = 3\n"));
    assert!(err.contains("unknown field `speed`"), "{err}");
    assert!(load("id = \"p\"\n").contains("missing field"));
}
