use screencapturekit::{
    sc_content_filter::{InitParams, SCContentFilter},
    sc_stream_configuration::SCStreamConfiguration,
    sc_shareable_content::SCShareableContent,
    sc_stream::SCStream,
    sc_output_handler::{SCStreamOutputType, SCStreamOutput},
};

#[tokio::main]
async fn main() {
    let content = SCShareableContent::current().await.unwrap();
    let display = content.displays.first().unwrap();
    let filter = SCContentFilter::new(InitParams::Display(display.clone()));
    let config = SCStreamConfiguration::new();
    config.set_captures_audio(true);
    config.set_excludes_current_process_audio(true);
    
    // How to capture audio?
}
