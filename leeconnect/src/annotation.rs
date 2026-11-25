//! Screen annotation stroke model (UU feature: annotation_v1 / annotation_v2).
//! Strokes are stored in normalized `[0.0..1.0]` remote screen coordinates and
//! quantized to `[u16; 2]` (`0..65535`) for ControlMsg transport.

#[derive(Debug, Clone)]
pub struct StrokePath {
    pub color: [u8; 4],
    pub width_px: f32,
    pub points: Vec<[f32; 2]>,
}

impl StrokePath {
    pub fn to_wire_points(&self) -> Vec<[u16; 2]> {
        self.points
            .iter()
            .map(|&[nx, ny]| {
                [
                    (nx.clamp(0.0, 1.0) * 65535.0) as u16,
                    (ny.clamp(0.0, 1.0) * 65535.0) as u16,
                ]
            })
            .collect()
    }

    pub fn from_wire(color: [u8; 4], width_px: u8, points: &[[u16; 2]]) -> Self {
        Self {
            color,
            width_px: (width_px as f32).max(1.0),
            points: points
                .iter()
                .map(|&[qx, qy]| [qx as f32 / 65535.0, qy as f32 / 65535.0])
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stroke_wire_quantization_roundtrip() {
        let s = StrokePath {
            color: [255, 59, 48, 255],
            width_px: 3.0,
            points: vec![[0.0, 0.0], [0.5, 0.25], [1.0, 1.0]],
        };
        let wire = s.to_wire_points();
        let back = StrokePath::from_wire(s.color, 3, &wire);
        assert_eq!(back.points.len(), 3);
        assert!((back.points[1][0] - 0.5).abs() < 1e-4);
        assert!((back.points[1][1] - 0.25).abs() < 1e-4);
    }
}
