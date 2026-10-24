//! The unpicked field, measured from the installed reference's active window.
//! These original polynomial gradients use sRGB components and window-relative
//! coordinates. They reproduce the default blue/purple material without copying
//! any texture or image asset, or inheriting a Space's obsolete single hue.
use browser_core::themes::ThemeColor;

const DARK: [[f32; 10]; 3] = [
    [
        12.91488, 1.63292, 2.48597, 21.18256, 26.81642, 10.38347, -10.68718, -19.39206, -13.02758,
        -5.07670,
    ],
    [
        12.26680, 11.15664, 5.75919, -15.73466, -16.23857, -6.96882, 4.53011, 10.52135, 3.56297,
        3.13728,
    ],
    [
        31.47765, 2.82330, 2.65978, -2.69344, -1.25224, -4.15148, -0.09342, 1.37175, -0.45457,
        1.98992,
    ],
];

const LIGHT: [[f32; 10]; 3] = [
    [
        226.22532, -0.08625, 13.81659, 37.13503, 38.45454, -0.80114, -19.17332, -31.16421,
        -14.76358, 1.41668,
    ],
    [
        226.68535, -3.86872, 7.87107, 21.99131, 21.81877, -4.25713, -11.05219, -19.82493, -6.77734,
        3.12772,
    ],
    [
        253.04009, -16.57450, -4.06263, 1.88919, -2.15707, -9.16694, 2.22902, -2.83273, 10.18104,
        5.59347,
    ],
];

pub(super) fn sample(dark: bool, x: f32, y: f32) -> ThemeColor {
    let (x, y) = (x.clamp(0., 1.), y.clamp(0., 1.));
    let terms = [
        1.,
        x,
        y,
        x * x,
        x * y,
        y * y,
        x * x * x,
        x * x * y,
        x * y * y,
        y * y * y,
    ];
    let coefficients = if dark { DARK } else { LIGHT };
    let channels = coefficients.map(|row| {
        row.into_iter()
            .zip(terms)
            .map(|(a, b)| a * b)
            .sum::<f32>()
            .clamp(0., 255.)
            / 255.
    });
    ThemeColor {
        red: channels[0],
        green: channels[1],
        blue: channels[2],
    }
}
