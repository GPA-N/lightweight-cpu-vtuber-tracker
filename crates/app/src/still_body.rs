//! Debug harness helpers for APP_STILL_BODY: landmark overlay and text summary of one photo.

use std::path::Path;

use avatar::body::HAND_SCORE_MIN;
use image::{Rgb, RgbImage};
use core::body::{SubjectSide, UpperBodyOutput};

const ARM_CHAINS: [[usize; 3]; 4] = [[11, 13, 15], [12, 14, 16], [15, 17, 19], [16, 18, 20]];
const HAND_BONES: [(usize, usize); 20] = [
    (0, 1), (1, 2), (2, 3), (3, 4),
    (0, 5), (5, 6), (6, 7), (7, 8),
    (0, 9), (9, 10), (10, 11), (11, 12),
    (0, 13), (13, 14), (14, 15), (15, 16),
    (0, 17), (17, 18), (18, 19), (19, 20),
];

fn dot(img: &mut RgbImage, x: f32, y: f32, r: i32, c: [u8; 3]) {
    for dy in -r..=r {
        for dx in -r..=r {
            let (px, py) = (x as i32 + dx, y as i32 + dy);
            if px >= 0 && py >= 0 && (px as u32) < img.width() && (py as u32) < img.height() {
                img.put_pixel(px as u32, py as u32, Rgb(c));
            }
        }
    }
}

fn line(img: &mut RgbImage, a: (f32, f32), b: (f32, f32), c: [u8; 3]) {
    let steps = ((b.0 - a.0).abs().max((b.1 - a.1).abs()) as i32).max(1);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        dot(img, a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, 0, c);
    }
}

/// Draws shoulders, arms (cyan when visible at `vis_min`, orange when not), and hand skeletons
/// (green when the hand score passes, red when it does not). Hips are never drawn.
/// With `mirror` the picture is flipped to match the mirrored camera preview.
pub fn overlay(rgb: &[u8], out: &UpperBodyOutput, vis_min: f32, mirror: bool, path: &Path) -> Result<(), String> {
    let mut img = RgbImage::from_raw(out.width, out.height, rgb.to_vec()).ok_or("bad image buffer")?;
    let p = |i: usize| (out.pose[i].x, out.pose[i].y);
    let vis = |i: usize| out.pose[i].visibility >= vis_min;
    line(&mut img, p(11), p(12), [255, 255, 0]);
    for chain in ARM_CHAINS {
        for w in chain.windows(2) {
            let c = if vis(w[0]) && vis(w[1]) { [0, 255, 255] } else { [255, 140, 0] };
            line(&mut img, p(w[0]), p(w[1]), c);
        }
    }
    for i in [11, 12, 13, 14, 15, 16, 17, 18, 19, 20] {
        let c = if vis(i) { [0, 255, 255] } else { [255, 140, 0] };
        dot(&mut img, p(i).0, p(i).1, 2, c);
    }
    for hand in &out.hands {
        let c = if hand.score >= HAND_SCORE_MIN { [0, 255, 0] } else { [255, 0, 0] };
        for (a, b) in HAND_BONES {
            line(&mut img, (hand.points[a][0], hand.points[a][1]), (hand.points[b][0], hand.points[b][1]), c);
        }
    }
    let img = if mirror { image::imageops::flip_horizontal(&img) } else { img };
    img.save(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// One summary line per fact, for the notes.
pub fn summary(out: &UpperBodyOutput, vis_min: f32) -> Vec<String> {
    let mut lines = vec![format!(
        "pose presence {:.3}; hand model {}",
        out.presence, out.hand_model
    )];
    for (name, i) in [
        ("L shoulder", 11), ("R shoulder", 12), ("L elbow", 13), ("R elbow", 14),
        ("L wrist", 15), ("R wrist", 16), ("L index", 19), ("R index", 20),
    ] {
        let q = out.pose[i];
        lines.push(format!(
            "  {name:<11} px=({:.0},{:.0}) z={:.0} vis={:.3}{}",
            q.x, q.y, q.z, q.visibility,
            if q.visibility >= vis_min { "" } else { " (below threshold: held at rest)" }
        ));
    }
    for h in &out.hands {
        let side = match h.side {
            SubjectSide::Left => "subject-left",
            SubjectSide::Right => "subject-right",
        };
        lines.push(format!(
            "  hand {side}: score {:.3} {} handedness-raw {:.3} [{}]",
            h.score,
            if h.score >= HAND_SCORE_MIN { "DETECTED" } else { "not detected" },
            h.handedness,
            out.hand_model
        ));
    }
    lines
}


/// Landmarks as JSON (source pixels), for comparison with the official MediaPipe reference.
pub fn dump_json(out: &UpperBodyOutput) -> String {
    let pose: Vec<String> = out
        .pose
        .iter()
        .map(|p| format!("[{:.2},{:.2},{:.2},{:.4}]", p.x, p.y, p.z, p.visibility))
        .collect();
    let hands: Vec<String> = out
        .hands
        .iter()
        .map(|h| {
            let pts: Vec<String> =
                h.points.iter().map(|p| format!("[{:.2},{:.2},{:.2}]", p[0], p[1], p[2])).collect();
            format!(
                "{{\"side\":\"{}\",\"score\":{:.4},\"handedness\":{:.4},\"points\":[{}]}}",
                if h.side == SubjectSide::Left { "left" } else { "right" },
                h.score,
                h.handedness,
                pts.join(",")
            )
        })
        .collect();
    format!(
        "{{\"width\":{},\"height\":{},\"presence\":{:.4},\"hand_model\":\"{}\",\"pose\":[{}],\"hands\":[{}]}}",
        out.width,
        out.height,
        out.presence,
        out.hand_model,
        pose.join(","),
        hands.join(",")
    )
}
