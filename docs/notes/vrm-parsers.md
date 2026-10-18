# VRM parser candidates (research, 2026-10-17)

Question: which crate parses VRM 0.x and 1.0 files without Bevy, so a model can be loaded with kiss3d (or any renderer) and drag-and-drop works across versions.

Method: crates.io and GitHub metadata via `gh api` (authenticated), Cargo.toml and source trees read from GitHub. No builds, no cargo, no edits to `crates/`. Compile cost is judged from Cargo.toml only. Anything not checked in this run is marked UNVERIFIED.

## Ranked table

| Rank | Crate | Repo | Licence | Last push / release | VRM 0.x | VRM 1.0 | Extensions covered | Bevy dep | Maintenance and popularity | Compile cost (Cargo.toml) |
|---|---|---|---|---|---|---|---|---|---|---|
| 1 | serde_vrm 0.5.0 | unavi-xyz/bevy_vrm | MIT OR Apache-2.0 (crate); repo Apache-2.0 | crates.io 2026-10-13 | VERIFIED (src/vrm0/mod.rs, migrate.rs) | VERIFIED (src/vrm1/*) | VRM (0.0), VRMC_vrm (meta, humanoid, expressions, firstPerson, lookAt), VRMC_springBone, VRMC_materials_mtoon, VRMC_node_constraint; 0.0 to 1.0 migration | Optional, behind feature `bevy` (off by default) | 7784 downloads, created 2024-03-19, same repo as bevy_vrm (67 stars) | Light: `serde` only by default |
| 2 | vrm-spec 0.1.1 | pixiv/vrm-utils-rs | Apache-2.0 (Cargo.toml VERIFIED); repo page showed no licence | 2026-06-15 | VERIFIED (src/vrm_0_0.rs: VRM0Schema, blend shapes, humanoid, first person, secondary animation, materials) | VERIFIED (src/vrmc_vrm_1_0/*, vrmc_spring_bone_1_0, vrmc_materials_mtoon_1_0, vrmc_vrm_animation_1_0) | VRMC_vrm, VRMC_springBone, VRMC_materials_mtoon, VRMA. No VRMC_node_constraint module (tree VERIFIED) | None | 6392 downloads, created 2024-07-12, 17 stars, from pixiv (official VRM vendor) | Light-medium: serde, serde_json; `gltf` and `rustc-hash` on by default (optional) |
| 3 | vrm-runtime 0.1.0 | nanowater/vrm-runtime-rs | MIT OR Apache-2.0 (crate); repo Apache-2.0 | 2026-08-18 (created same day) | NO (description: VRM 1.0 only) | VERIFIED by description and tree (constraint, spring_bone, look_at, expressions, material, first_person, skin, animation) | Full VRM 1.0 runtime, depends on vrm-spec | None | 82 downloads, 1 star, very new. Maintenance UNVERIFIED | Heavy-ish: gltf (with features), image (jpeg, png), glam, mikktspace, base64, vrm-spec |
| 4 | bevy_vrm 0.5.0 | unavi-xyz/bevy_vrm | MIT OR Apache-2.0 | crates.io 2026-10-13 | VERIFIED (README: "Loads VRM 0.0 and 1.0 avatars"; src/extensions/vrm0.rs present) | VERIFIED (README, src/extensions/vrm1.rs) | Humanoid, MToon, first person, spring bones. Not yet: expressions, look-at, node constraints, spring colliders (README) | Yes: bevy, bevy_shader_mtoon, gltf, serde_vrm | 20333 downloads, 67 stars | Very heavy (full Bevy) |
| 5 | bevy_vrm1 0.10.0 | not-elm/bevy_vrm1 | MIT OR Apache-2.0 (crate); repo Apache-2.0 | crates.io 2026-10-03 | NO (README: "This crate only supports VRM 1.0.") | VERIFIED | Spring bone, look-at, VRMA, node constraint, first person | Yes: bevy, indexmap, windows | 7409 downloads, 39 stars, created 2025-06-01 | Very heavy (full Bevy). This is the app's current loader |
| 6 | bevy_shader_mtoon 0.5.0 | unavi-xyz/bevy_vrm | MIT OR Apache-2.0 | crates.io 2026-10-13 | n/a (shader only) | n/a | MToon shader | Yes: bevy, bitflags | 16680 downloads | Very heavy. Not a parser |
| 7 | gltf_kun_vrm 0.3.0 | unavi-xyz/bevy_vrm | MIT OR Apache-2.0 | 2026-04-07 | UNVERIFIED | UNVERIFIED | Extension for gltf_kun, not a standalone parser | None listed in this run | 7256 downloads | UNVERIFIED (not read) |
| 8 | bevy_mtoon 0.0.0 | thebutlah | UNVERIFIED | 2023-08-28 | n/a | n/a | Stale | Yes | 1629 downloads | Not recommended |

Reference sources (not crates):
- vrm-c/vrm-specification: 340 stars, pushed 2026-10-02. Contains official JSON schemas: `specification/0.0/schema/*.json`, `VRMC_vrm-1.0/schema`, `VRMC_springBone-1.0/schema`, `VRMC_springBone_limit-1.0`, `VRMC_springBone_extended_collider-1.0`, `VRMC_node_constraint-1.0/schema`, `VRMC_materials_mtoon-1.0/schema`, `VRMC_materials_hdr_emissiveMultiplier-1.0`. VERIFIED by tree. Licence UNVERIFIED (not shown on repo page).
- pixiv/three-vrm: 2220 stars, pushed 2026-10-16, MIT (from lean-libs.md, not re-read). Reference for 0.x to 1.0 mapping only. UNVERIFIED in this run.
- V-Sekai/godot-vrm: 477 stars, pushed 2026-07-08. GitHub reports NOASSERTION, LICENSE file MIT (from lean-libs.md, UNVERIFIED here).

## Findings

- serde_vrm is the only candidate found that covers VRM 0.x and VRM 1.0 in one parser-only crate with a migration step, and it makes Bevy optional. The lib.rs description says: "VRM 0.0 values convert into their VRM 1.0 equivalents." The 0.0 BoneName to 1.0 HumanBoneName conversion shifts thumbs (0.0 ThumbProximal maps to 1.0 ThumbMetacarpal, in migrate.rs).
- serde_vrm's own tests (tests/vrm1_parse.rs) use only the seed_san and constraint_twist fixtures, both VRM 1.0. VRM 0.x parsing is in the source but UNVERIFIED by a test in this run.
- vrm-spec has the widest VRM 0.0 struct set (VRM0Schema, blend shape groups, binds, secondary animation) and official pixiv authorship. It has no node constraint types.
- vrm-runtime is the only renderer-agnostic runtime (constraints, spring bones, look-at, expressions) but is VRM 1.0 only and too new to trust without testing.
- Bevy-based bevy_vrm loads 0.x and 1.0 (README), but its README lists expressions, look-at, node constraints and spring colliders as not yet supported. Bevy coupling makes it unusable for a kiss3d app.
- The app's current failures (vrm-compat.md) are 0.x rejection by bevy_vrm1 (design, VRM 1.0 only) and the Twist stall. Neither is fixed by swapping the parser alone. The parser only supplies data. Spring, look-at and constraint simulation still need code on the renderer side, or vrm-runtime.

## Recommended approach

1. Use serde_vrm 0.5.0 with default features (no `bevy` feature) as the parser for both 0.x and 1.0. Pull the VRM extension JSON out of the glTF document (gltf crate or raw JSON), then deserialize into `serde_vrm::vrm0::Vrm` or `serde_vrm::vrm1::VrmcVrm`, and convert 0.x with its migrate path so the renderer sees one 1.0-shaped model.
2. Fallback if serde_vrm 0.x coverage fails a test: use vrm-spec 0.1.1 for the 0.x structs (`vrm_0_0::VRM0Schema`) and keep serde_vrm for 1.0.
3. Do not hand-write a full parser. If a struct is missing, generate types from the official vrm-c JSON schemas (typify or schemars) and pin the schema commit.
4. Keep the renderer-side work (spring bones, look-at, constraints, MToon) as a separate step. Consider vrm-runtime only for its 1.0 simulation code after a licence and test check.

## Effort

- Parser integration for 1.0 (serde_vrm, no bevy): about 0.5 to 1 day, including a fixture test on seed_san and constraint_twist.
- 0.x path (migrate, Godette and Devil fixtures): about 1 day, including tests for the thumb shift and blend shape to expression mapping.
- Simulation (spring bones, look-at, constraints) on kiss3d: several days to weeks. This is the bulk of the work, and the parser choice does not reduce it.

## Risks

- serde_vrm is pre-1.0 (0.5.0), with 7784 downloads. API churn is likely. Pin the version.
- 0.x to 1.0 migration fidelity is UNVERIFIED for expressions and first person. Test on Godette and Devil before relying on it.
- vrm-spec's repo page showed no licence. The crate's Cargo.toml says Apache-2.0 (VERIFIED). Confirm before vendoring.
- vrm-runtime is 2 months old with 82 downloads. Do not depend on it without reading its tests.
- Parser crates do not render MToon or simulate spring bones. Those are separate costs.
- Several facts in this note rely on earlier runs: lean-libs.md, vrm-compat.md and kiss3d-trial.md (kiss3d skin and morph verified in spike; no VRMC_vrm, MToon or spring parser in kiss3d).
