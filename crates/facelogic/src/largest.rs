#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceBox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

fn area(b: &FaceBox) -> f32 {
    if b.w > 0.0 && b.h > 0.0 {
        b.w * b.h
    } else {
        0.0
    }
}

pub fn largest(boxes: &[FaceBox]) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (i, b) in boxes.iter().enumerate() {
        let a = area(b);
        match best {
            Some((_, best_area)) if a <= best_area => {}
            _ => best = Some((i, a)),
        }
    }
    best.map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(w: f32, h: f32) -> FaceBox {
        FaceBox { x: 0.0, y: 0.0, w, h }
    }

    #[test]
    fn largest_zero_boxes_is_none() {
        assert_eq!(largest(&[]), None);
    }

    #[test]
    fn largest_one_box_is_zero() {
        assert_eq!(largest(&[b(2.0, 3.0)]), Some(0));
    }

    #[test]
    fn largest_three_boxes_picks_biggest_area() {
        let boxes = [b(1.0, 1.0), b(4.0, 2.0), b(3.0, 3.0)];
        assert_eq!(largest(&boxes), Some(2));
    }

    #[test]
    fn largest_tie_picks_lowest_index() {
        let boxes = [b(2.0, 2.0), b(4.0, 1.0), b(1.0, 4.0)];
        assert_eq!(largest(&boxes), Some(0));
    }

    #[test]
    fn largest_nan_or_negative_counts_as_zero() {
        let boxes = [b(f32::NAN, 2.0), b(-3.0, 2.0), b(0.5, 0.5)];
        assert_eq!(largest(&boxes), Some(2));
    }
}
