use super::themes::ColorTheme;
use indicatif::{ProgressBar, ProgressStyle as IndicatifStyle, MultiProgress};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug, Clone)]
pub enum ProgressStyle {
    Spinner,
    Bar,
    Percentage,
    Bytes,
    Files,
}

pub struct ProgressIndicator {
    colors_enabled: bool,
    pub theme: ColorTheme,
    multi_progress: Arc<MultiProgress>,
}

impl ProgressIndicator {
    pub fn new(colors_enabled: bool, theme: ColorTheme) -> Self {
        Self {
            colors_enabled,
            theme,
            multi_progress: Arc::new(MultiProgress::new()),
        }
    }

    /// Show a thinking indicator with animated dots
    pub async fn show_thinking(&self, message: &str) {
        let progress_bar = self.create_spinner();
        
        if self.colors_enabled {
            progress_bar.set_message(self.theme.format_thinking(message).to_string());
        } else {
            progress_bar.set_message(format!("Thinking: {}", message));
        }
        
        for _ in 0..10 {
            progress_bar.tick();
            sleep(Duration::from_millis(100)).await;
        }
        
        progress_bar.finish_and_clear();
    }

    /// Create a file scanning progress bar
    pub fn create_scan_progress(&self, total_files: u64) -> ScanProgress {
        let progress_bar = self.multi_progress.add(ProgressBar::new(total_files));
        
        let style = if self.colors_enabled {
            IndicatifStyle::default_bar()
                .template("{prefix:.cyan.bold} [{bar:40.green/blue}] {pos}/{len} {msg} ({eta})")
                .unwrap()
                .progress_chars("█░░")
        } else {
            IndicatifStyle::default_bar()
                .template("{prefix} [{bar:40}] {pos}/{len} {msg} ({eta})")
                .unwrap()
                .progress_chars("##-")
        };
        
        progress_bar.set_style(style);
        progress_bar.set_prefix("📁 Scanning");
        
        ScanProgress {
            progress_bar,
            colors_enabled: self.colors_enabled,
            theme: self.theme.clone(),
        }
    }

    /// Create an analysis progress bar
    pub fn create_analysis_progress(&self, total_files: u64) -> AnalysisProgress {
        let progress_bar = self.multi_progress.add(ProgressBar::new(total_files));
        
        let style = if self.colors_enabled {
            IndicatifStyle::default_bar()
                .template("{prefix:.cyan.bold} [{bar:40.magenta/blue}] {pos}/{len} {msg} ({eta})")
                .unwrap()
                .progress_chars("█░░")
        } else {
            IndicatifStyle::default_bar()
                .template("{prefix} [{bar:40}] {pos}/{len} {msg} ({eta})")
                .unwrap()
                .progress_chars("=>-")
        };
        
        progress_bar.set_style(style);
        progress_bar.set_prefix("🔍 Analyzing");
        
        AnalysisProgress {
            progress_bar,
            colors_enabled: self.colors_enabled,
            theme: self.theme.clone(),
        }
    }

    /// Create a download/upload progress bar for bytes
    pub fn create_bytes_progress(&self, total_bytes: u64, operation: &str) -> BytesProgress {
        let progress_bar = self.multi_progress.add(ProgressBar::new(total_bytes));
        
        let style = if self.colors_enabled {
            IndicatifStyle::default_bar()
                .template("{prefix:.cyan.bold} [{bar:40.yellow/blue}] {bytes}/{total_bytes} {msg} ({bytes_per_sec}, {eta})")
                .unwrap()
                .progress_chars("█░░")
        } else {
            IndicatifStyle::default_bar()
                .template("{prefix} [{bar:40}] {bytes}/{total_bytes} {msg} ({bytes_per_sec}, {eta})")
                .unwrap()
                .progress_chars("##-")
        };
        
        progress_bar.set_style(style);
        progress_bar.set_prefix(operation.to_string());
        
        BytesProgress {
            progress_bar,
            colors_enabled: self.colors_enabled,
            theme: self.theme.clone(),
        }
    }

    /// Create a generic spinner for indeterminate operations
    pub fn create_spinner(&self) -> ProgressBar {
        let spinner = self.multi_progress.add(ProgressBar::new_spinner());
        
        let style = if self.colors_enabled {
            IndicatifStyle::default_spinner()
                .template("{prefix:.cyan.bold} {spinner:.blue} {msg}")
                .unwrap()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
        } else {
            IndicatifStyle::default_spinner()
                .template("{prefix} {spinner} {msg}")
                .unwrap()
                .tick_strings(&["|", "/", "-", "\\"])
        };
        
        spinner.set_style(style);
        spinner.enable_steady_tick(Duration::from_millis(100));
        
        spinner
    }

    /// Create a multi-step progress indicator
    pub fn create_multi_step(&self, steps: Vec<String>) -> MultiStepProgress {
        MultiStepProgress::new(steps, self.colors_enabled, self.theme.clone(), self.multi_progress.clone())
    }
}

pub struct ScanProgress {
    progress_bar: ProgressBar,
    colors_enabled: bool,
    theme: ColorTheme,
}

impl ScanProgress {
    pub fn set_message(&self, msg: &str) {
        if self.colors_enabled {
            self.progress_bar.set_message(self.theme.format_muted(msg).to_string());
        } else {
            self.progress_bar.set_message(msg.to_string());
        }
    }

    pub fn inc(&self, delta: u64) {
        self.progress_bar.inc(delta);
    }

    pub fn inc_with_message(&self, delta: u64, msg: &str) {
        self.inc(delta);
        self.set_message(msg);
    }

    pub fn finish_with_message(&self, msg: &str) {
        if self.colors_enabled {
            self.progress_bar.finish_with_message(self.theme.format_success(msg).to_string());
        } else {
            self.progress_bar.finish_with_message(format!("[OK] {}", msg));
        }
    }

    pub fn abandon_with_message(&self, msg: &str) {
        if self.colors_enabled {
            self.progress_bar.abandon_with_message(self.theme.format_error(msg).to_string());
        } else {
            self.progress_bar.abandon_with_message(format!("[ERROR] {}", msg));
        }
    }

    pub fn set_position(&self, pos: u64) {
        self.progress_bar.set_position(pos);
    }
}

impl Drop for ScanProgress {
    fn drop(&mut self) {
        self.progress_bar.finish_and_clear();
    }
}

pub struct AnalysisProgress {
    progress_bar: ProgressBar,
    colors_enabled: bool,
    theme: ColorTheme,
}

impl AnalysisProgress {
    pub fn set_current_file(&self, file_path: &str) {
        let msg = if self.colors_enabled {
            self.theme.format_file_path(file_path).to_string()
        } else {
            file_path.to_string()
        };
        self.progress_bar.set_message(msg);
    }

    pub fn inc(&self) {
        self.progress_bar.inc(1);
    }

    pub fn inc_with_file(&self, file_path: &str) {
        self.inc();
        self.set_current_file(file_path);
    }

    pub fn finish_with_message(&self, msg: &str) {
        if self.colors_enabled {
            self.progress_bar.finish_with_message(self.theme.format_success(msg).to_string());
        } else {
            self.progress_bar.finish_with_message(format!("[OK] {}", msg));
        }
    }

    pub fn abandon_with_message(&self, msg: &str) {
        if self.colors_enabled {
            self.progress_bar.abandon_with_message(self.theme.format_error(msg).to_string());
        } else {
            self.progress_bar.abandon_with_message(format!("[ERROR] {}", msg));
        }
    }
}

impl Drop for AnalysisProgress {
    fn drop(&mut self) {
        self.progress_bar.finish_and_clear();
    }
}

pub struct BytesProgress {
    progress_bar: ProgressBar,
    colors_enabled: bool,
    theme: ColorTheme,
}

impl BytesProgress {
    pub fn set_message(&self, msg: &str) {
        self.progress_bar.set_message(msg.to_string());
    }

    pub fn inc(&self, delta: u64) {
        self.progress_bar.inc(delta);
    }

    pub fn finish_with_message(&self, msg: &str) {
        if self.colors_enabled {
            self.progress_bar.finish_with_message(self.theme.format_success(msg).to_string());
        } else {
            self.progress_bar.finish_with_message(format!("[OK] {}", msg));
        }
    }

    pub fn abandon_with_message(&self, msg: &str) {
        if self.colors_enabled {
            self.progress_bar.abandon_with_message(self.theme.format_error(msg).to_string());
        } else {
            self.progress_bar.abandon_with_message(format!("[ERROR] {}", msg));
        }
    }
}

impl Drop for BytesProgress {
    fn drop(&mut self) {
        self.progress_bar.finish_and_clear();
    }
}

pub struct MultiStepProgress {
    steps: Vec<String>,
    current_step: usize,
    progress_bar: ProgressBar,
    colors_enabled: bool,
    theme: ColorTheme,
}

impl MultiStepProgress {
    fn new(steps: Vec<String>, colors_enabled: bool, theme: ColorTheme, multi_progress: Arc<MultiProgress>) -> Self {
        let total_steps = steps.len() as u64;
        let progress_bar = multi_progress.add(ProgressBar::new(total_steps));
        
        let style = if colors_enabled {
            IndicatifStyle::default_bar()
                .template("{prefix:.cyan.bold} [{bar:40.green/blue}] Step {pos}/{len}: {msg}")
                .unwrap()
                .progress_chars("█░░")
        } else {
            IndicatifStyle::default_bar()
                .template("{prefix} [{bar:40}] Step {pos}/{len}: {msg}")
                .unwrap()
                .progress_chars("##-")
        };
        
        progress_bar.set_style(style);
        progress_bar.set_prefix("🚀 Processing");
        
        // Set initial message
        if let Some(first_step) = steps.first() {
            progress_bar.set_message(first_step.clone());
        }
        
        Self {
            steps,
            current_step: 0,
            progress_bar,
            colors_enabled,
            theme,
        }
    }

    pub fn next_step(&mut self) -> Option<&str> {
        if self.current_step < self.steps.len() {
            let step = &self.steps[self.current_step];
            
            if self.colors_enabled {
                self.progress_bar.set_message(self.theme.format_highlight(step).to_string());
            } else {
                self.progress_bar.set_message(step.clone());
            }
            
            self.progress_bar.inc(1);
            self.current_step += 1;
            
            Some(step)
        } else {
            None
        }
    }

    pub fn set_step_message(&self, msg: &str) {
        if self.colors_enabled {
            self.progress_bar.set_message(self.theme.format_muted(msg).to_string());
        } else {
            self.progress_bar.set_message(msg.to_string());
        }
    }

    pub fn finish_with_message(&self, msg: &str) {
        if self.colors_enabled {
            self.progress_bar.finish_with_message(self.theme.format_success(msg).to_string());
        } else {
            self.progress_bar.finish_with_message(format!("[OK] {}", msg));
        }
    }

    pub fn abandon_with_message(&self, msg: &str) {
        if self.colors_enabled {
            self.progress_bar.abandon_with_message(self.theme.format_error(msg).to_string());
        } else {
            self.progress_bar.abandon_with_message(format!("[ERROR] {}", msg));
        }
    }

    pub fn current_step_index(&self) -> usize {
        self.current_step
    }

    pub fn total_steps(&self) -> usize {
        self.steps.len()
    }

    pub fn is_complete(&self) -> bool {
        self.current_step >= self.steps.len()
    }
}

impl Drop for MultiStepProgress {
    fn drop(&mut self) {
        self.progress_bar.finish_and_clear();
    }
} 