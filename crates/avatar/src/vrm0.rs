//! VRM 0.x support: a 0.x file is converted to VRM 1.0 before it is loaded, because the loader
//! (`bevy_vrm1`) reads only the 1.0 `VRMC_vrm` extension.
//!
//! What is converted: the model's name and license fields, the humanoid bone map (including the
//! renamed thumb bones), the expressions (blend shape groups with their morph binds), the look-at
//! settings, the spring bones (hair and cloth, as chains with a short tail node and sphere colliders)
//! and the facing direction (0.x models face -z, 1.0 models face +z, so the scene gets a root node
//! turned 180 degrees about y).
//!
//! What is not converted: MToon shading (the plain glTF materials in the file are used instead).
//! The caller tells the user.

use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::errors::{validate_vrm1, AvatarError};

const JSON_CHUNK: u32 = 0x4E4F_534A;
const BIN_CHUNK: u32 = 0x004E_4942;

/// Message shown with a converted model.
pub const CONVERTED_NOTE: &str =
    "VRM 0.x model converted to 1.0; MToon shading is not converted";

/// A glTF binary split into its JSON and binary chunks.
pub struct Glb {
    pub json: Value,
    pub bin: Option<Vec<u8>>,
}

pub fn parse_glb(bytes: &[u8]) -> Result<Glb, String> {
    if bytes.len() < 20 || &bytes[0..4] != b"glTF" {
        return Err("not a glTF binary file".into());
    }
    let (mut pos, mut json, mut bin) = (12usize, None, None);
    while pos + 8 <= bytes.len() {
        let len = u32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]) as usize;
        let kind = u32::from_le_bytes([bytes[pos + 4], bytes[pos + 5], bytes[pos + 6], bytes[pos + 7]]);
        let (start, end) = (pos + 8, pos + 8 + len);
        let chunk = bytes.get(start..end).ok_or("truncated glTF chunk")?;
        match kind {
            JSON_CHUNK => json = Some(serde_json::from_slice(chunk).map_err(|e| format!("invalid JSON chunk: {e}"))?),
            BIN_CHUNK if bin.is_none() => bin = Some(chunk.to_vec()),
            _ => {}
        }
        pos = end;
    }
    Ok(Glb { json: json.ok_or("missing JSON chunk")?, bin })
}

pub fn write_glb(glb: &Glb) -> Vec<u8> {
    let mut json = serde_json::to_vec(&glb.json).unwrap_or_default();
    while json.len() % 4 != 0 {
        json.push(b' ');
    }
    let mut bin = glb.bin.clone().unwrap_or_default();
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let bin_part = if glb.bin.is_some() { 8 + bin.len() } else { 0 };
    let total = 12 + 8 + json.len() + bin_part;
    let mut out = b"glTF".to_vec();
    out.extend(2u32.to_le_bytes());
    out.extend((total as u32).to_le_bytes());
    out.extend((json.len() as u32).to_le_bytes());
    out.extend(JSON_CHUNK.to_le_bytes());
    out.extend(json);
    if glb.bin.is_some() {
        out.extend((bin.len() as u32).to_le_bytes());
        out.extend(BIN_CHUNK.to_le_bytes());
        out.extend(bin);
    }
    out
}

/// True when the glTF JSON carries the 0.x `VRM` extension and not the 1.0 `VRMC_vrm` one.
pub fn is_vrm0(json: &Value) -> bool {
    let ext = &json["extensions"];
    !ext["VRM"].is_null() && ext["VRMC_vrm"].is_null()
}

/// 0.x humanoid bone name to its 1.0 name. Only the thumb differs: 0.x has proximal, intermediate
/// and distal; 1.0 has metacarpal, proximal and distal.
fn bone_name(old: &str) -> String {
    for side in ["left", "right"] {
        for (from, to) in [("Proximal", "Metacarpal"), ("Intermediate", "Proximal")] {
            if old == format!("{side}Thumb{from}") {
                return format!("{side}Thumb{to}");
            }
        }
    }
    old.to_string()
}

/// 0.x blend shape preset name (lower case) to the 1.0 expression name.
fn expression_name(preset: &str) -> Option<&'static str> {
    Some(match preset {
        "neutral" => "neutral",
        "a" => "aa",
        "i" => "ih",
        "u" => "ou",
        "e" => "ee",
        "o" => "oh",
        "blink" => "blink",
        "blink_l" => "blinkLeft",
        "blink_r" => "blinkRight",
        "joy" => "happy",
        "angry" => "angry",
        "sorrow" => "sad",
        "fun" => "relaxed",
        "lookup" => "lookUp",
        "lookdown" => "lookDown",
        "lookleft" => "lookLeft",
        "lookright" => "lookRight",
        _ => return None,
    })
}

fn text(v: &Value, keys: &[&str]) -> String {
    keys.iter().find_map(|k| v[*k].as_str()).unwrap_or("").to_string()
}

fn convert_meta(m: &Value) -> Value {
    let license = text(m, &["licenseName"]);
    let allow = |keys: &[&str]| text(m, keys) == "Allow";
    let mut meta = Map::new();
    meta.insert("name".into(), json!(text(m, &["title"])));
    meta.insert("version".into(), json!(text(m, &["version"])));
    meta.insert("authors".into(), json!([text(m, &["author"])]));
    meta.insert(
        "avatarPermission".into(),
        json!(match text(m, &["allowedUserName"]).as_str() {
            "Everyone" => "everyone",
            "ExplicitlyLicensedPerson" => "onlySeparatelyLicensedPerson",
            _ => "onlyAuthor",
        }),
    );
    meta.insert(
        "commercialUsage".into(),
        json!(if allow(&["commercialUssageName", "commercialUsageName"]) { "corporation" } else { "personalNonProfit" }),
    );
    meta.insert("creditNotation".into(), json!(if license == "CC0" { "unnecessary" } else { "required" }));
    meta.insert("modification".into(), json!(if license.contains("ND") { "prohibited" } else { "allowModification" }));
    // Redistribution is allowed only for the Creative Commons names; anything else counts as prohibited.
    meta.insert("allowRedistribution".into(), json!(license.starts_with("CC")));
    meta.insert("allowExcessivelyViolentUsage".into(), json!(allow(&["violentUssageName", "violentUsageName"])));
    meta.insert("allowExcessivelySexualUsage".into(), json!(allow(&["sexualUssageName", "sexualUsageName"])));
    meta.insert("allowAntisocialOrHateUsage".into(), json!(false));
    meta.insert("allowPoliticalOrReligiousUsage".into(), json!(false));
    let url = text(m, &["otherLicenseUrl"]);
    if !url.is_empty() {
        meta.insert("licenseUrl".into(), json!(url));
    }
    Value::Object(meta)
}

fn convert_humanoid(h: &Value) -> Value {
    let mut bones = Map::new();
    for b in h["humanBones"].as_array().into_iter().flatten() {
        let (Some(name), Some(node)) = (b["bone"].as_str(), b["node"].as_i64()) else { continue };
        if node >= 0 {
            bones.insert(bone_name(name), json!({ "node": node }));
        }
    }
    json!({ "humanBones": bones })
}

/// Expressions from the blend shape groups. Morph binds name a mesh in 0.x and a node in 1.0, so the
/// first node that uses the mesh stands in; weights go from 0..100 to 0..1.
fn convert_expressions(master: &Value, mesh_node: &std::collections::HashMap<u64, usize>) -> Value {
    let mut preset = Map::new();
    for g in master["blendShapeGroups"].as_array().into_iter().flatten() {
        let Some(name) = g["presetName"].as_str().and_then(|p| expression_name(&p.to_ascii_lowercase())) else {
            continue;
        };
        if preset.contains_key(name) {
            continue;
        }
        let binds: Vec<Value> = g["binds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|b| {
                let node = mesh_node.get(&b["mesh"].as_u64()?)?;
                Some(json!({ "node": node, "index": b["index"].as_u64()?, "weight": b["weight"].as_f64().unwrap_or(100.0) / 100.0 }))
            })
            .collect();
        preset.insert(name.to_string(), json!({ "isBinary": g["isBinary"].as_bool().unwrap_or(false), "morphTargetBinds": binds }));
    }
    // Many 0.x models have one `blink` shape and no left or right ones. Without a fallback their eyes
    // would never blink, so the single blink also stands in for each side (a wink closes both eyes).
    if let Some(blink) = preset.get("blink").cloned() {
        for side in ["blinkLeft", "blinkRight"] {
            preset.entry(side.to_string()).or_insert_with(|| blink.clone());
        }
    }
    json!({ "preset": preset })
}

fn range_map(v: &Value) -> Value {
    json!({ "inputMaxValue": v["xRange"].as_f64().unwrap_or(90.0), "outputScale": v["yRange"].as_f64().unwrap_or(10.0) })
}

fn convert_look_at(first_person: &Value) -> Value {
    let offset = first_person["firstPersonBoneOffset"]
        .as_object()
        .map(|o| [o.get("x"), o.get("y"), o.get("z")].map(|c| c.and_then(Value::as_f64).unwrap_or(0.0)))
        .unwrap_or([0.0, 0.06, 0.0]);
    json!({
        "type": if first_person["lookAtTypeName"].as_str() == Some("Bone") { "bone" } else { "expression" },
        "offsetFromHeadBone": offset,
        "rangeMapHorizontalInner": range_map(&first_person["lookAtHorizontalInner"]),
        "rangeMapHorizontalOuter": range_map(&first_person["lookAtHorizontalOuter"]),
        "rangeMapVerticalDown": range_map(&first_person["lookAtVerticalDown"]),
        "rangeMapVerticalUp": range_map(&first_person["lookAtVerticalUp"]),
    })
}

/// Length of the tail node added after the last bone of a chain (1.0 chains need a tail; 0.x bones
/// simulate their own tip). Same 7 cm that UniVRM's migration uses.
const TAIL_LENGTH: f64 = 0.07;

fn vec3(v: &Value) -> Option<[f64; 3]> {
    Some([v["x"].as_f64()?, v["y"].as_f64()?, v["z"].as_f64()?])
}

fn node_ok(nodes: &[Value], i: i64) -> Option<usize> {
    (i >= 0 && (i as usize) < nodes.len()).then_some(i as usize)
}

/// Spring chains below one 0.x root bone. The first child continues the chain; every other child
/// starts a chain of its own. A chain that ends on a leaf gets a tail node so the leaf can swing.
fn spring_chains(nodes: &mut Vec<Value>, root: usize, seen: &mut std::collections::HashSet<usize>) -> Vec<Vec<usize>> {
    let mut chains = Vec::new();
    let mut stack = vec![(root, Vec::<usize>::new())];
    while let Some((start, mut chain)) = stack.pop() {
        let mut node = start;
        loop {
            if !seen.insert(node) {
                break; // a node can sit in one chain only (and a cycle ends here)
            }
            chain.push(node);
            let children: Vec<usize> = nodes[node]["children"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|c| c.as_i64().and_then(|c| node_ok(nodes, c)))
                .collect();
            let Some((&first, rest)) = children.split_first() else {
                let t = nodes[node]["translation"].as_array().map(|t| [0, 1, 2].map(|i| t.get(i).and_then(Value::as_f64).unwrap_or(0.0)));
                let t = t.unwrap_or([0.0; 3]);
                let len = (t[0] * t[0] + t[1] * t[1] + t[2] * t[2]).sqrt();
                let dir = if len > 1e-6 { [t[0] / len, t[1] / len, t[2] / len] } else { [0.0, 1.0, 0.0] };
                nodes.push(json!({ "name": "vrm0_spring_tail", "translation": [dir[0] * TAIL_LENGTH, dir[1] * TAIL_LENGTH, dir[2] * TAIL_LENGTH] }));
                let tail = nodes.len() - 1;
                match nodes[node]["children"].as_array_mut() {
                    Some(c) => c.push(json!(tail)),
                    None => nodes[node]["children"] = json!([tail]),
                }
                chain.push(tail);
                break;
            };
            for &c in rest {
                stack.push((c, Vec::new()));
            }
            node = first;
        }
        if chain.len() > 1 {
            chains.push(chain);
        }
    }
    chains
}

/// Converts the 0.x secondary animation (hair, cloth) to `VRMC_springBone`. Colliders become spheres,
/// the gravity direction is turned with the root node (a world vector, so x and z change sign), and
/// the parameters are copied as they are.
fn convert_spring_bone(v0: &Value, nodes: &mut Vec<Value>) -> Option<Value> {
    let sa = &v0["secondaryAnimation"];
    let groups = sa["colliderGroups"].as_array().cloned().unwrap_or_default();
    let bone_groups = sa["boneGroups"].as_array().cloned().unwrap_or_default();
    if groups.is_empty() && bone_groups.is_empty() {
        return None;
    }
    let (mut colliders, mut collider_groups) = (Vec::new(), Vec::new());
    for g in &groups {
        let mut idx = Vec::new();
        if let Some(node) = g["node"].as_i64().and_then(|n| node_ok(nodes, n)) {
            for c in g["colliders"].as_array().into_iter().flatten() {
                let (Some(offset), Some(radius)) = (vec3(&c["offset"]), c["radius"].as_f64()) else { continue };
                idx.push(colliders.len());
                colliders.push(json!({ "node": node, "shape": { "sphere": { "offset": offset, "radius": radius } } }));
            }
        }
        collider_groups.push(json!({ "colliders": idx }));
    }
    let mut springs = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (gi, g) in bone_groups.iter().enumerate() {
        let gravity = vec3(&g["gravityDir"]).map(|d| [-d[0], d[1], -d[2]]).unwrap_or([0.0, -1.0, 0.0]);
        let joint = |node: usize| {
            json!({
                "node": node,
                "stiffness": g["stiffiness"].as_f64().unwrap_or(1.0),
                "gravityPower": g["gravityPower"].as_f64().unwrap_or(0.0),
                "gravityDir": gravity,
                "dragForce": g["dragForce"].as_f64().unwrap_or(0.4),
                "hitRadius": g["hitRadius"].as_f64().unwrap_or(0.02),
            })
        };
        let groups_used: Vec<usize> = g["colliderGroups"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| c.as_u64().map(|c| c as usize).filter(|c| *c < collider_groups.len()))
            .collect();
        let name = g["comment"].as_str().filter(|s| !s.is_empty()).map(str::to_string).unwrap_or_else(|| format!("spring{gi}"));
        let center = g["center"].as_i64().and_then(|n| node_ok(nodes, n));
        let roots: Vec<usize> = g["bones"].as_array().into_iter().flatten().filter_map(|b| b.as_i64().and_then(|b| node_ok(nodes, b))).collect();
        for root in roots {
            for chain in spring_chains(nodes, root, &mut seen) {
                let mut spring = json!({ "name": name, "joints": chain.iter().map(|n| joint(*n)).collect::<Vec<_>>() });
                if !groups_used.is_empty() {
                    spring["colliderGroups"] = json!(groups_used);
                }
                if let Some(c) = center {
                    spring["center"] = json!(c);
                }
                springs.push(spring);
            }
        }
    }
    Some(json!({ "specVersion": "1.0", "colliders": colliders, "colliderGroups": collider_groups, "springs": springs }))
}

/// Rewrites the glTF JSON of a 0.x model into a 1.0 model.
pub fn convert_json(json: &mut Value) -> Result<(), String> {
    let v0 = json["extensions"]["VRM"].clone();
    if v0.is_null() {
        return Err("no VRM 0.x extension".into());
    }
    let mut mesh_node = std::collections::HashMap::new();
    for (i, n) in json["nodes"].as_array().into_iter().flatten().enumerate() {
        if let Some(m) = n["mesh"].as_u64() {
            mesh_node.entry(m).or_insert(i);
        }
    }

    let mut vrmc = json!({
        "specVersion": "1.0",
        "meta": convert_meta(&v0["meta"]),
        "humanoid": convert_humanoid(&v0["humanoid"]),
        "expressions": convert_expressions(&v0["blendShapeMaster"], &mesh_node),
    });
    if !v0["firstPerson"].is_null() {
        vrmc["lookAt"] = convert_look_at(&v0["firstPerson"]);
    }

    // 0.x models face -z; 1.0 models face +z. A new root node turned 180 degrees about y carries the scene.
    let scene = json["scene"].as_u64().unwrap_or(0) as usize;
    let roots = json["scenes"][scene]["nodes"].clone();
    let nodes = json["nodes"].as_array_mut().ok_or("no nodes")?;
    let spring_bone = convert_spring_bone(&v0, nodes);
    nodes.push(json!({ "name": "vrm0_flip", "rotation": [0.0, 1.0, 0.0, 0.0], "children": roots }));
    let flip = nodes.len() - 1;
    json["scenes"][scene]["nodes"] = json!([flip]);

    let ext = json["extensions"].as_object_mut().ok_or("no extensions")?;
    ext.remove("VRM");
    ext.insert("VRMC_vrm".into(), vrmc);
    let has_spring = spring_bone.is_some();
    if let Some(s) = spring_bone {
        ext.insert("VRMC_springBone".into(), s);
    }
    if let Some(used) = json["extensionsUsed"].as_array_mut() {
        used.retain(|e| e.as_str() != Some("VRM"));
        for name in ["VRMC_vrm", "VRMC_springBone"] {
            if (name == "VRMC_vrm" || has_spring) && !used.iter().any(|e| e.as_str() == Some(name)) {
                used.push(json!(name));
            }
        }
    }
    Ok(())
}

/// Converts a 0.x glTF binary to a 1.0 one.
pub fn convert_glb(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut glb = parse_glb(bytes)?;
    convert_json(&mut glb.json)?;
    Ok(write_glb(&glb))
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ *b as u64).wrapping_mul(0x0100_0000_01b3))
}

/// A model that can be loaded: the file to hand to the loader and notes to show the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenedModel {
    pub path: PathBuf,
    pub notes: Vec<String>,
}

/// Checks `path` and, for a VRM 0.x file, converts it to a 1.0 file in the temp folder. The original
/// is never changed.
pub fn open_model(path: &Path) -> Result<OpenedModel, AvatarError> {
    if let Ok(bytes) = std::fs::read(path)
        && let Ok(glb) = parse_glb(&bytes)
        && is_vrm0(&glb.json)
    {
        let not_vrm1 = |reason: String| AvatarError::NotVrm1 { path: path.to_path_buf(), reason };
        let converted = convert_glb(&bytes).map_err(|e| not_vrm1(format!("VRM 0.x conversion failed: {e}")))?;
        let dir = std::env::temp_dir().join("vtuber-tracker-vrm0");
        std::fs::create_dir_all(&dir).map_err(|e| not_vrm1(format!("cannot create {}: {e}", dir.display())))?;
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let out = dir.join(format!("{stem}-{:016x}.vrm", fnv1a(&bytes)));
        std::fs::write(&out, converted).map_err(|e| not_vrm1(format!("cannot write {}: {e}", out.display())))?;
        validate_vrm1(&out)?;
        return Ok(OpenedModel { path: out, notes: vec![CONVERTED_NOTE.to_string()] });
    }
    validate_vrm1(path)?;
    Ok(OpenedModel { path: path.to_path_buf(), notes: vec![] })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Value {
        json!({
            "asset": { "version": "2.0" },
            "scene": 0,
            "scenes": [{ "nodes": [0] }],
            "nodes": [
                { "name": "root", "children": [1, 2] },
                { "name": "hips" },
                { "name": "face", "mesh": 0 }
            ],
            "extensionsUsed": ["VRM"],
            "extensions": { "VRM": {
                "meta": { "title": "Test", "author": "Someone", "allowedUserName": "Everyone",
                          "licenseName": "CC_BY", "commercialUssageName": "Allow" },
                "humanoid": { "humanBones": [
                    { "bone": "hips", "node": 1 },
                    { "bone": "leftThumbProximal", "node": 5 },
                    { "bone": "leftThumbIntermediate", "node": 6 },
                    { "bone": "leftThumbDistal", "node": 7 },
                    { "bone": "head", "node": -1 }
                ] },
                "blendShapeMaster": { "blendShapeGroups": [
                    { "presetName": "A", "binds": [{ "mesh": 0, "index": 3, "weight": 100 }] },
                    { "presetName": "Blink_L", "binds": [{ "mesh": 0, "index": 4, "weight": 50 }], "isBinary": true },
                    { "presetName": "Joy", "binds": [{ "mesh": 9, "index": 0, "weight": 100 }] },
                    { "presetName": "Unknown", "name": "custom", "binds": [] }
                ] },
                "firstPerson": { "lookAtTypeName": "Bone", "firstPersonBoneOffset": { "x": 0, "y": 0.07, "z": 0 },
                                 "lookAtHorizontalOuter": { "xRange": 80, "yRange": 12 } }
            } }
        })
    }

    fn with_hair() -> Value {
        let mut j = sample();
        j["nodes"] = json!([
            { "name": "root", "children": [1, 2, 5] },
            { "name": "hair", "children": [3, 6] },
            { "name": "face", "mesh": 0 },
            { "name": "hair2", "children": [4] },
            { "name": "hair3", "translation": [0.0, -0.1, 0.0] },
            { "name": "head" },
            { "name": "fringe", "translation": [0.2, 0.0, 0.0] }
        ]);
        j["extensions"]["VRM"]["secondaryAnimation"] = json!({
            "boneGroups": [{ "comment": "hair", "stiffiness": 0.8, "gravityPower": 0.3,
                             "gravityDir": { "x": 0.0, "y": -1.0, "z": 0.5 }, "dragForce": 0.5,
                             "center": -1, "hitRadius": 0.02, "bones": [1], "colliderGroups": [0] }],
            "colliderGroups": [{ "node": 5, "colliders": [{ "offset": { "x": 0.0, "y": 0.1, "z": 0.0 }, "radius": 0.08 }] }]
        });
        j
    }

    #[test]
    fn hair_chains_get_tails_and_the_parameters_are_copied() {
        let mut j = with_hair();
        convert_json(&mut j).unwrap();
        let sb = &j["extensions"]["VRMC_springBone"];
        let springs = sb["springs"].as_array().unwrap();
        assert_eq!(springs.len(), 2, "the second child starts a chain of its own: {sb}");
        let nodes_of = |s: &Value| s["joints"].as_array().unwrap().iter().map(|x| x["node"].as_u64().unwrap()).collect::<Vec<_>>();
        let first = nodes_of(&springs[0]);
        assert_eq!(&first[..3], &[1, 3, 4], "{first:?}");
        assert_eq!(first.len(), 4, "a tail follows the leaf");
        let second = nodes_of(&springs[1]);
        assert_eq!(second[0], 6);
        assert_eq!(second.len(), 2);
        let tail = &j["nodes"][first[3] as usize];
        assert_eq!(j["nodes"][4]["children"], json!([first[3]]));
        assert!((tail["translation"][1].as_f64().unwrap() + TAIL_LENGTH).abs() < 1e-9, "the tail continues the bone's direction: {tail}");
        let joint = &springs[0]["joints"][0];
        assert_eq!(joint["stiffness"], 0.8);
        assert_eq!(joint["dragForce"], 0.5);
        assert_eq!(joint["gravityPower"], 0.3);
        assert_eq!(joint["hitRadius"], 0.02);
        assert_eq!(joint["gravityDir"], json!([-0.0, -1.0, -0.5]), "x and z turn with the 180 degree root");
        assert_eq!(springs[0]["colliderGroups"], json!([0]));
        assert!(springs[0].get("center").is_none());
    }

    #[test]
    fn colliders_become_spheres_on_their_node() {
        let mut j = with_hair();
        convert_json(&mut j).unwrap();
        let sb = &j["extensions"]["VRMC_springBone"];
        assert_eq!(sb["colliders"][0], json!({ "node": 5, "shape": { "sphere": { "offset": [0.0, 0.1, 0.0], "radius": 0.08 } } }));
        assert_eq!(sb["colliderGroups"][0]["colliders"], json!([0]));
        let used = j["extensionsUsed"].as_array().unwrap();
        assert!(used.contains(&json!("VRMC_springBone")) && used.contains(&json!("VRMC_vrm")));
    }

    #[test]
    fn a_model_without_secondary_animation_gets_no_spring_extension() {
        let mut j = sample();
        convert_json(&mut j).unwrap();
        assert!(j["extensions"].get("VRMC_springBone").is_none());
        assert!(!j["extensionsUsed"].as_array().unwrap().contains(&json!("VRMC_springBone")));
    }

    #[test]
    fn a_0x_model_is_recognised_and_a_1x_model_is_not() {
        assert!(is_vrm0(&sample()));
        assert!(!is_vrm0(&json!({ "extensions": { "VRMC_vrm": {} } })));
        assert!(!is_vrm0(&json!({})));
    }

    #[test]
    fn humanoid_bones_move_across_and_thumbs_are_renamed() {
        let mut j = sample();
        convert_json(&mut j).unwrap();
        let bones = &j["extensions"]["VRMC_vrm"]["humanoid"]["humanBones"];
        assert_eq!(bones["hips"]["node"], 1);
        assert_eq!(bones["leftThumbMetacarpal"]["node"], 5);
        assert_eq!(bones["leftThumbProximal"]["node"], 6);
        assert_eq!(bones["leftThumbDistal"]["node"], 7);
        assert!(bones["head"].is_null(), "a bone with node -1 is not present");
    }

    #[test]
    fn expressions_use_nodes_and_unit_weights() {
        let mut j = sample();
        convert_json(&mut j).unwrap();
        let e = &j["extensions"]["VRMC_vrm"]["expressions"]["preset"];
        assert_eq!(e["aa"]["morphTargetBinds"][0], json!({ "node": 2, "index": 3, "weight": 1.0 }));
        assert_eq!(e["blinkLeft"]["morphTargetBinds"][0]["weight"], 0.5);
        assert_eq!(e["blinkLeft"]["isBinary"], true);
        assert_eq!(e["happy"]["morphTargetBinds"], json!([]), "a bind on an unknown mesh is dropped");
        assert!(e.get("custom").is_none() && e.as_object().unwrap().len() == 3, "{e}");
        assert!(e.get("blink").is_none(), "no blink shape, so no blink fallback either");
    }

    #[test]
    fn a_single_blink_shape_also_serves_each_eye() {
        let mut j = sample();
        j["extensions"]["VRM"]["blendShapeMaster"]["blendShapeGroups"] =
            json!([{ "presetName": "Blink", "binds": [{ "mesh": 0, "index": 1, "weight": 100 }] }]);
        convert_json(&mut j).unwrap();
        let e = &j["extensions"]["VRMC_vrm"]["expressions"]["preset"];
        assert_eq!(e["blink"], e["blinkLeft"]);
        assert_eq!(e["blink"], e["blinkRight"]);
        // A model that has its own left shape keeps it.
        let mut j = sample();
        j["extensions"]["VRM"]["blendShapeMaster"]["blendShapeGroups"] = json!([
            { "presetName": "Blink", "binds": [{ "mesh": 0, "index": 1, "weight": 100 }] },
            { "presetName": "Blink_L", "binds": [{ "mesh": 0, "index": 2, "weight": 100 }] }
        ]);
        convert_json(&mut j).unwrap();
        let e = &j["extensions"]["VRMC_vrm"]["expressions"]["preset"];
        assert_eq!(e["blinkLeft"]["morphTargetBinds"][0]["index"], 2);
        assert_eq!(e["blinkRight"]["morphTargetBinds"][0]["index"], 1);
    }

    #[test]
    fn look_at_keeps_type_offset_and_ranges() {
        let mut j = sample();
        convert_json(&mut j).unwrap();
        let l = &j["extensions"]["VRMC_vrm"]["lookAt"];
        assert_eq!(l["type"], "bone");
        assert_eq!(l["offsetFromHeadBone"][1], 0.07);
        assert_eq!(l["rangeMapHorizontalOuter"], json!({ "inputMaxValue": 80.0, "outputScale": 12.0 }));
        assert_eq!(l["rangeMapVerticalUp"], json!({ "inputMaxValue": 90.0, "outputScale": 10.0 }), "defaults fill the gaps");
    }

    #[test]
    fn the_scene_gets_a_root_turned_half_a_circle_about_y() {
        let mut j = sample();
        convert_json(&mut j).unwrap();
        let flip = j["scenes"][0]["nodes"][0].as_u64().unwrap() as usize;
        assert_eq!(j["nodes"][flip]["rotation"], json!([0.0, 1.0, 0.0, 0.0]));
        assert_eq!(j["nodes"][flip]["children"], json!([0]));
    }

    #[test]
    fn meta_and_extension_lists_are_rewritten() {
        let mut j = sample();
        convert_json(&mut j).unwrap();
        let m = &j["extensions"]["VRMC_vrm"]["meta"];
        assert_eq!(m["name"], "Test");
        assert_eq!(m["authors"], json!(["Someone"]));
        assert_eq!(m["avatarPermission"], "everyone");
        assert_eq!(m["commercialUsage"], "corporation");
        assert_eq!(m["allowRedistribution"], true);
        assert_eq!(j["extensionsUsed"], json!(["VRMC_vrm"]));
        assert!(j["extensions"]["VRM"].is_null());
        assert!(!is_vrm0(&j));
        assert_eq!(j["extensions"]["VRMC_vrm"]["specVersion"], "1.0");
    }

    #[test]
    fn a_glb_survives_write_and_parse_and_converts_to_a_valid_1x_file() {
        let glb = Glb { json: sample(), bin: Some(vec![1, 2, 3]) };
        let bytes = write_glb(&glb);
        let back = parse_glb(&bytes).unwrap();
        assert_eq!(back.json, glb.json);
        assert_eq!(&back.bin.unwrap()[..3], &[1, 2, 3]);

        let path = std::env::temp_dir().join(format!("avatar_vrm0_{}.vrm", std::process::id()));
        std::fs::write(&path, &bytes).unwrap();
        let opened = open_model(&path).expect("0.x file opens");
        let _ = std::fs::remove_file(&path);
        assert_ne!(opened.path, path, "the converted copy is a different file");
        assert_eq!(opened.notes, vec![CONVERTED_NOTE.to_string()]);
        assert_eq!(validate_vrm1(&opened.path), Ok(()));
        let _ = std::fs::remove_file(&opened.path);
    }

    #[test]
    fn a_1x_file_is_passed_through_and_text_is_refused() {
        let ok = std::env::temp_dir().join(format!("avatar_vrm1_{}.vrm", std::process::id()));
        let glb = Glb { json: json!({ "asset": { "version": "2.0" }, "extensions": { "VRMC_vrm": { "specVersion": "1.0" } } }), bin: None };
        std::fs::write(&ok, write_glb(&glb)).unwrap();
        let opened = open_model(&ok).unwrap();
        let _ = std::fs::remove_file(&ok);
        assert_eq!(opened.path, ok);
        assert!(opened.notes.is_empty());

        let bad = std::env::temp_dir().join(format!("avatar_text_{}.vrm", std::process::id()));
        std::fs::write(&bad, b"hello").unwrap();
        let err = open_model(&bad);
        let _ = std::fs::remove_file(&bad);
        assert!(matches!(err, Err(AvatarError::NotVrm1 { .. })));
    }
}
