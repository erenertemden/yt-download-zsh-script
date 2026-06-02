#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Screen {
    Form,
    OutputPicker,
    Running,
    Done,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Focus {
    Url,
    SourceFormat,
    Resolution,
    Format,
    Convert,
    Encoder,
    DeleteOriginal,
    Output,
    Start,
}

impl Focus {
    pub fn next(self) -> Self {
        match self {
            Self::Url => Self::SourceFormat,
            Self::SourceFormat => Self::Resolution,
            Self::Resolution => Self::Format,
            Self::Format => Self::Convert,
            Self::Convert => Self::Encoder,
            Self::Encoder => Self::DeleteOriginal,
            Self::DeleteOriginal => Self::Output,
            Self::Output => Self::Start,
            Self::Start => Self::Url,
        }
    }

    pub fn previous(self) -> Self {
        match self {
            Self::Url => Self::Start,
            Self::SourceFormat => Self::Url,
            Self::Resolution => Self::SourceFormat,
            Self::Format => Self::Resolution,
            Self::Convert => Self::Format,
            Self::Encoder => Self::Convert,
            Self::DeleteOriginal => Self::Encoder,
            Self::Output => Self::DeleteOriginal,
            Self::Start => Self::Output,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AvailableFormat {
    pub id: String,
    pub label: String,
    pub ext: String,
    pub vcodec: String,
    pub acodec: String,
    pub height: Option<u32>,
    pub fps_label: Option<String>,
    pub has_audio: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncoderMode {
    AppleHardware,
    CpuX264,
}

impl Default for EncoderMode {
    fn default() -> Self {
        if cfg!(target_os = "macos") {
            Self::AppleHardware
        } else {
            Self::CpuX264
        }
    }
}

impl EncoderMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::AppleHardware => "Fast Apple Hardware",
            Self::CpuX264 => "Smaller CPU x264",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::AppleHardware => Self::CpuX264,
            Self::CpuX264 => Self::AppleHardware,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Progress {
    pub stage: String,
    pub ratio: Option<f64>,
    pub detail: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaylistProgress {
    pub current: usize,
    pub total: usize,
}

#[derive(Debug)]
pub enum WorkerEvent {
    Log(String),
    Progress(Progress),
    Playlist(PlaylistProgress),
    Done { success: bool, message: String },
}
