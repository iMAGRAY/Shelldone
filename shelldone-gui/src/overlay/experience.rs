use crate::experience::adapters::TerminalOverlayRenderer;
use crate::experience::ports::ExperienceRendererPort;
use crate::experience::ExperienceViewModel;
use mux::termwiztermtab::TermWizTerminal;
use std::sync::Arc;
use std::time::Duration;
use termwiz::cell::{AttributeChange, CellAttributes, Intensity};
use termwiz::color::ColorAttribute;
use termwiz::input::{InputEvent, KeyCode, KeyEvent};
use termwiz::surface::Change;
use termwiz::terminal::Terminal;

pub fn show_experience_hub(
    mut term: TermWizTerminal,
    view_model: Arc<ExperienceViewModel>,
) -> anyhow::Result<()> {
    term.no_grab_mouse_in_raw_mode();

    render_frame(&mut term, &view_model)?;

    loop {
        match term.poll_input(Some(Duration::from_millis(500)))? {
            Some(InputEvent::Key(KeyEvent {
                key: KeyCode::Escape | KeyCode::Enter,
                ..
            }))
            | Some(InputEvent::Key(KeyEvent {
                key: KeyCode::Char('q'),
                ..
            })) => break,
            Some(InputEvent::Key(KeyEvent {
                key: KeyCode::Char('r'),
                ..
            })) => {
                render_frame(&mut term, &view_model)?;
            }
            Some(InputEvent::Resized { .. }) => {
                render_frame(&mut term, &view_model)?;
            }
            _ => {}
        }
    }

    Ok(())
}

fn render_frame(
    term: &mut TermWizTerminal,
    view_model: &ExperienceViewModel,
) -> anyhow::Result<()> {
    let renderer = TerminalOverlayRenderer::new();
    let frame = renderer.compose(view_model);

    let mut changes: Vec<Change> = vec![Change::ClearScreen(ColorAttribute::Default)];

    changes.push(AttributeChange::Intensity(Intensity::Bold).into());
    changes.push(Change::Text(format!("{}\r\n", frame.headline)));
    changes.push(AttributeChange::Intensity(Intensity::Normal).into());
    changes.push(Change::Text("\r\n".to_string()));

    for block in frame.blocks {
        changes.push(AttributeChange::Intensity(Intensity::Bold).into());
        changes.push(Change::Text(format!("{}\r\n", block.title)));
        changes.push(AttributeChange::Intensity(Intensity::Normal).into());
        if let Some(subtitle) = block.subtitle {
            let mut attrs = CellAttributes::default();
            attrs.set_italic(true);
            changes.push(Change::AllAttributes(attrs));
            changes.push(Change::Text(format!("{}\r\n", subtitle)));
            changes.push(Change::AllAttributes(CellAttributes::default()));
        }
        for body in block.body_lines {
            changes.push(Change::Text(format!("   {}\r\n", body)));
        }
        changes.push(Change::Text("\r\n".to_string()));
    }

    changes.push(AttributeChange::Intensity(Intensity::Bold).into());
    changes.push(Change::Text(format!("{}\r\n", frame.footer)));
    changes.push(Change::AllAttributes(CellAttributes::default()));
    changes.push(AttributeChange::Intensity(Intensity::Normal).into());
    changes.push(Change::Text(
        "Press Esc/Enter/Q to close · R to refresh\r\n".to_string(),
    ));

    term.render(&changes)?;
    Ok(())
}
