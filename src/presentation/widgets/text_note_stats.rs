use ratatui::prelude::*;
use thousands::Separable;

pub struct TextNoteStats {
    reactions: usize,
    reposts: usize,
    zap_sats: u64,
}

impl TextNoteStats {
    pub fn new(reactions: usize, reposts: usize, zap_sats: u64) -> Self {
        Self {
            reactions,
            reposts,
            zap_sats,
        }
    }
}

impl From<TextNoteStats> for Text<'_> {
    fn from(value: TextNoteStats) -> Self {
        Line::from(vec![
            Span::styled(
                format!("{}Likes", value.reactions.separate_with_commas()),
                Style::default().fg(Color::LightRed),
            ),
            Span::raw(" "),
            Span::styled(
                format!("{}Reposts", value.reposts.separate_with_commas()),
                Style::default().fg(Color::LightGreen),
            ),
            Span::raw(" "),
            Span::styled(
                format!("{}Sats", value.zap_sats.separate_with_commas()),
                Style::default().fg(Color::LightYellow),
            ),
        ])
        .into()
    }
}
