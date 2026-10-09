/// Test function that builds a DOM from the musicxml_example.mxml file
#[test]
fn test_build_musicxml_dom() {
    use musicxml::read_score_partwise;
    // Path to the example MusicXML file
    let file_path =
        "/home/levitanus/gits/reaper-levitanus/reaper-levitanus/src/score/musicxml_example.musicxml";

    // Read the MusicXML file into a ScorePartwise DOM
    let score = read_score_partwise(file_path).expect("Failed to read MusicXML file");

    println!("{:#?}", score);
}
