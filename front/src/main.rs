use reaper_levitanus::ffmpeg::front;

fn main() {
    env_logger::try_init().ok();
    front().unwrap()
}
