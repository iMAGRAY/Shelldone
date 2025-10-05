use crate::experience::ExperienceViewModel;

#[derive(Clone, Debug)]
pub struct ExperienceUiBlock {
    pub title: String,
    pub subtitle: Option<String>,
    pub body_lines: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ExperienceUiFrame {
    pub headline: String,
    pub blocks: Vec<ExperienceUiBlock>,
    pub footer: String,
}

pub trait ExperienceRendererPort {
    fn compose(&self, view_model: &ExperienceViewModel) -> ExperienceUiFrame;
}
