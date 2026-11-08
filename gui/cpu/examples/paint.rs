use std::{io::Write, time::Duration};

use blit::{Frame, FrameInfo, Input, LogicalSize};
use blit_cpu::{Renderer, RendererConfig, Scanline, VecBuffer, Xrgb8888, paint::Painter};
use blit_gui::{BoundsClip, TextConfig, TextSystem, atom, color::Color, shadow::BoxShadow, style::BorderRadius};

blit_cpu::paint_handlers! {
    struct Effects {
        shadow: BoxShadow => blit_cpu::shadow::Renderer,
    }
}

fn main() -> std::io::Result<()> {
    let renderer =
        Renderer::new(VecBuffer::<Xrgb8888>::new(160, 100), RendererConfig::default()).strategy(Scanline::default());
    let text = TextSystem::new(TextConfig::default(), blit_text_parley::Backend::without_system_fonts()).unwrap();
    let mut painter = Painter::new(
        renderer,
        text,
        Effects {
            shadow: blit_cpu::shadow::Renderer::default(),
        },
    );
    let mut frame = Frame::default();
    frame.build(
        &mut painter,
        FrameInfo::new(LogicalSize::new(160.0, 100.0)),
        Duration::ZERO,
        Input::None,
        |ui: blit::Ui<'_, _>| {
            let mut panel = ui.layout(()).clip(BoundsClip);
            panel.insert(atom::Rectangle::new().background(Color::rgb(65, 120, 170)));
            panel.insert(
                atom::Shadow::new(Color::rgba(0, 0, 0, 180))
                    .radius(BorderRadius::uniform(12.0))
                    .blur(12.0)
                    .spread(4.0)
                    .inset(true),
            );
        },
    );
    frame.layout(&mut painter);
    frame.paint(&mut painter);
    painter.render(0);

    let path = std::env::args().nth(1).unwrap_or_else(|| "/tmp/blit-paint.ppm".into());
    let mut output = std::io::BufWriter::new(std::fs::File::create(&path)?);
    write!(output, "P6\n160 100\n255\n")?;
    for pixel in painter.buffer().pixels() {
        let pixel = pixel.raw();
        output.write_all(&[(pixel >> 16) as u8, (pixel >> 8) as u8, pixel as u8])?;
    }
    println!("rendered rectangle and shadow atoms through Paint to {path}");
    Ok(())
}
