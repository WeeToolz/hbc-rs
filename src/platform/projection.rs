//! HBC projection in pixel coordinates, with the C scene depth preserved.

// C HBC config.h: VIEW_Z_ORIGIN. gfx_init uses guOrtho with near=100, far=1000.
pub(super) const SCENE_Z: f32 = -420.0;

pub(super) fn projection(width: u16, height: u16) -> [[f32; 4]; 4] {
    let inverse_depth = 1.0 / (1000.0 - 100.0);
    [
        [2.0 / f32::from(width), 0.0, 0.0, -1.0],
        [0.0, -2.0 / f32::from(height), 0.0, 1.0],
        [0.0, 0.0, -inverse_depth, -1000.0 * inverse_depth],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transform(matrix: [[f32; 4]; 4], vertex: [f32; 3]) -> [f32; 3] {
        core::array::from_fn(|row| {
            matrix[row][0] * vertex[0]
                + matrix[row][1] * vertex[1]
                + matrix[row][2] * vertex[2]
                + matrix[row][3]
        })
    }

    #[test]
    fn scene_depth_matches_c_orthographic_projection_inside_clip_volume() {
        // Independent C contract: (-(-420) - 1000) / (1000 - 100) = -580/900.
        let clip = transform(projection(640, 480), [320.0, 240.0, SCENE_Z]);
        assert_eq!(SCENE_Z, -420.0);
        assert!(clip[0].abs() < 0.000001);
        assert!(clip[1].abs() < 0.000001);
        assert!((clip[2] - (-0.644_444_47)).abs() < 0.000001);
        assert!(clip[2] > -1.0 && clip[2] < 0.0);
    }

    #[test]
    fn pixel_coordinates_match_c_centered_coordinates_for_both_aspects() {
        for width in [640, 852] {
            let matrix = projection(width, 480);
            for (vertex, expected) in [
                ([0.0, 0.0, SCENE_Z], [-1.0, 1.0]),
                ([f32::from(width), 480.0, SCENE_Z], [1.0, -1.0]),
            ] {
                let clip = transform(matrix, vertex);
                assert!((clip[0] - expected[0]).abs() < 0.000001);
                assert!((clip[1] - expected[1]).abs() < 0.000001);
            }
        }
    }
}
