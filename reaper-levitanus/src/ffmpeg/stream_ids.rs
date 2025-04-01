use std::collections::HashMap;

#[derive(Debug)]
pub struct StreamIds {
    ids: Vec<&'static str>,
    index: usize,
}
impl StreamIds {
    fn next(&mut self) -> &'static str {
        self.index += 1;
        if self.index >= self.ids.len() {
            self.index = 0
        };
        self.ids[self.index].clone()
    }
    fn new() -> Self {
        Self {
            ids: vec![
                "Bam", "Nah", "Yea", "Yep", "Naw", "Hey", "Yay", "Nay", "Pow", "Wow", "Moo", "Boo",
                "Bye", "Yum", "Ugh", "Bah", "Umm", "Why", "Aha", "Aye", "Hmm", "Hah", "Huh", "Ssh",
                "Brr", "Heh", "Oop", "Oof", "Zzz", "Gee", "Grr", "Yup", "Gah", "Mmm", "Arr", "Eww",
                "Ehh", "Ace", "Aid", "Aim", "Air", "Ale", "Arm", "Art", "Awl", "Eel", "Ear", "Era",
                "Ice", "Ire", "Ilk", "Oar", "Oak", "Oat", "Oil", "Ore", "Owl", "Urn", "Web", "Cab",
                "Dab", "Jab", "Lab", "Tab", "Dad", "Fad", "Lad", "Mad", "Bag", "Gag", "Hag", "Lag",
                "Mag", "Rag", "Tag", "Pal", "Cam", "Dam", "Fam", "Ham", "Jam", "Ram", "Ban", "Can",
                "Fan", "Man", "Pan", "Tan", "Bap", "Cap", "Lap", "Pap", "Rap", "Sap", "Tap", "Yap",
                "Bar", "Car", "Jar", "Tar", "War", "Bat", "Cat", "Hat", "Mat", "Pat", "Tat", "Rat",
                "Vat", "Caw", "Jaw", "Law", "Maw", "Paw", "Bay", "Cay", "Day", "Hay", "Ray", "Pay",
                "Way", "Max", "Sax", "Tax", "Pea", "Sea", "Tea", "Bed", "Med", "Leg", "Peg", "Bee",
                "Lee", "Tee", "Gem", "Bet", "Jet", "Net", "Pet", "Set", "Den", "Hen", "Men", "Pen",
                "Ten", "Yen", "Dew", "Mew", "Pew", "Bib", "Fib", "Jib", "Rib", "Sib", "Bid", "Kid",
                "Lid", "Vid", "Tie", "Lie", "Pie", "Fig", "Jig", "Pig", "Rig", "Wig", "Dim", "Bin",
                "Din", "Fin", "Gin", "Pin", "Sin", "Tin", "Win", "Yin", "Dip", "Lip", "Pip", "Sip",
                "Tip", "Git", "Hit", "Kit", "Pit", "Wit", "Bod", "Cod", "God", "Mod", "Pod", "Rod",
                "Doe", "Foe", "Hoe", "Roe", "Toe", "Bog", "Cog", "Dog", "Fog", "Hog", "Jog", "Log",
                "Poi", "Con", "Son", "Ton", "Zoo", "Cop", "Hop", "Mop", "Pop", "Top", "Bot", "Cot",
                "Dot", "Lot", "Pot", "Tot", "Bow", "Cow", "Sow", "Row", "Box", "Lox", "Pox", "Boy",
                "Soy", "Toy", "Cub", "Nub", "Pub", "Sub", "Tub", "Bug", "Hug", "Jug", "Mug", "Rug",
                "Tug", "Bum", "Gum", "Hum", "Rum", "Tum", "Bun", "Gun", "Pun", "Run", "Sun", "Cup",
                "Pup", "Cut", "Gut", "Hut", "Nut", "Rut", "Egg", "Ego", "Elf", "Elm", "Emu", "End",
                "Era", "Eve", "Eye", "Ink", "Inn", "Ion", "Ivy", "Lye", "Dye", "Rye", "Pus", "Gym",
                "Her", "His", "Him", "Our", "You", "She", "Add", "Are", "Eat", "Oil", "Use", "Nab",
                "Jab", "Bag", "Lag", "Nag", "Rag", "Sag", "Tag", "Wag", "Jam", "Ram", "Tan", "Cap",
                "Lap", "Nap", "Rap", "Sap", "Tap", "Yap", "Mar", "Has", "Was", "Pat", "Lay", "Pay",
                "Say", "Tax", "See", "Get", "Let", "Net", "Met", "Pet", "Set", "Wet", "Mew", "Sew",
                "Lie", "Tie", "Bog", "Jog", "Boo", "Coo", "Moo", "Bop", "Hop", "Lop", "Mop", "Pop",
                "Top", "Sop", "Bow", "Mow", "Row", "Tow", "Dub", "Rub", "Lug", "Tug", "Hum", "Sup",
                "Buy", "Jot", "Rot", "Nod", "Hem", "Wed", "Fib", "Jib", "Rib", "Did", "Dig", "Jig",
                "Rig", "Dip", "Nip", "Sip", "Rip", "Zip", "Gin", "Win", "Bit", "Hit", "Sit", "Pry",
                "Try", "Cry", "All", "Fab", "Bad", "Mad", "Far", "Fat", "Raw", "Lax", "Gay", "Big",
                "Dim", "Fit", "Red", "Wet", "Old", "New", "Hot", "Coy", "Fun", "Ill", "Odd", "Shy",
                "Dry", "Wry", "And", "But", "Yet", "For", "Nor", "Not", "How", "Too", "Yet", "Now",
                "Off", "Any", "Out", "area", "army", "baby", "back", "ball", "band", "bank",
                "base", "bill", "body", "book", "call", "card", "care", "case", "cash", "city",
                "club", "cost", "date", "deal", "door", "duty", "East", "Edge", "Face", "Fact",
                "Farm", "Fear", "File", "Film", "Fire", "Firm", "Fish", "Food", "Foot", "Form",
                "Fund", "Game", "Girl", "Goal", "Gold", "Hair", "Half", "Hall", "Hand", "Head",
                "Help", "Hill", "Home", "Hope", "Hour", "Idea", "Jack", "John", "Kind", "King",
                "Lack", "Lady", "Land", "Life", "Line", "List", "Look", "Lord", "Loss", "Love",
                "Mark", "Mary", "Mind", "Miss", "Move", "Name", "Need", "News", "Note", "Page",
                "Pain", "Pair", "Park", "Part", "Past", "Path", "Paul", "Plan", "Play", "Post",
                "Race", "Rain", "Rate", "Rest", "Rise", "Risk", "Road", "Rock", "Role", "Room",
                "Rule", "Sale", "Seat", "Shop", "Show", "Side", "Sign", "Site", "Size", "Skin",
                "Sort", "Star", "Step", "Task", "Team", "Term", "Test", "Text", "Time", "Tour",
                "Town", "Tree", "Turn", "Type", "Unit", "User", "View", "Wall", "Week", "West",
                "Wife", "Will", "Wind", "Wine", "Wood", "Word", "Work", "Year", "Feel", "Hern",
                "Hers", "Many", "Mine", "Mine", "Much", "Nada", "None", "Nowt", "Ours", "Same",
                "Self", "Some", "Such", "That", "Thee", "Them", "They", "This", "Thon", "Thor",
                "Thou", "Thou", "What", "When", "Whom", "Yere", "Your", "bear", "beat", "blow",
                "burn", "call", "care", "cast", "come", "cook", "cope", "cost", "dare", "deal",
                "deny", "draw", "drop", "Earn", "Face", "Fail", "Fall", "Fear", "Feel", "Fill",
                "Find", "Form", "Gain", "Give", "Grow", "Hang", "Hate", "Have", "Head", "Hear",
                "Help", "Hide", "Hold", "Hope", "Hurt", "Join", "Jump", "Keep", "Kill", "Know",
                "Land", "Last", "Lead", "Lend", "Lift", "Like", "Link", "Live", "Look", "Lose",
                "Love", "Make", "Mark", "Meet", "Mind", "Miss", "Move", "Must", "Name", "Need",
                "Note", "Open", "Pass", "Pick", "Plan", "Play", "Pray", "Pull", "Push", "Read",
                "Rely", "Rest", "Ride", "Ring", "Rise", "Risk", "Roll", "Rule", "Save", "Seek",
                "Seem", "Sell", "Send", "Shed", "Show", "Shut", "Sign", "Sing", "Slip", "Sort",
                "Stay", "Step", "Stop", "Suit", "Take", "Talk", "Tell", "Tend", "Test", "Turn",
                "Vary", "View", "Vote", "Wait", "Wake", "Walk", "Want", "Warn", "Wash", "Wear",
                "Will", "Wish", "Work", "able", "back", "bare", "bass", "blue", "bold", "busy",
                "calm", "cold", "cool", "damp", "dark", "dead", "deaf", "dear", "deep", "dual",
                "dull", "dumb", "Easy", "Evil", "Fair", "Fast", "Fine", "Firm", "Flat", "Fond",
                "Foul", "Free", "Full", "Glad", "Good", "Grey", "Grim", "Half", "Hard", "Head",
                "High", "Holy", "Huge", "Just", "Keen", "Kind", "Last", "Late", "Lazy", "Like",
                "Live", "Lone", "Long", "Loud", "Main", "Male", "Mass", "Mean", "Mere", "Mild",
                "Nazi", "Near", "Neat", "Next", "Nice", "Okay", "Only", "Open", "Oral", "Pale",
                "Past", "Pink", "Poor", "Pure", "Rare", "Real", "Rear", "Rich", "Rude", "Safe",
                "Same", "Sick", "Slim", "Slow", "Soft", "Sole", "Sore", "Sure", "Tall", "Then",
                "Thin", "Tidy", "Tiny", "Tory", "True", "Ugly", "Vain", "Vast", "Very", "Vice",
                "Warm", "Wary", "Weak", "Wide", "Wild", "Wise", "Zero", "both", "Else", "Ergo",
                "Lest", "Like", "Once", "Only", "Plus", "Save", "Sith", "Than", "That", "Then",
                "Thou", "Till", "Unto", "When", "Some", "ably", "afar", "anew", "away", "back",
                "damn", "dead", "deep", "down", "duly", "Easy", "Else", "Even", "Ever", "Fair",
                "Fast", "Flat", "Full", "Good", "Half", "Hard", "Here", "High", "Home", "Idly",
                "Just", "Late", "Like", "Live", "Long", "Loud", "Much", "Near", "Nice", "Okay",
                "Once", "Only", "Over", "Part", "Past", "Real", "Slow", "Solo", "Soon", "Sure",
                "That", "Then", "This", "Thus", "Very", "When", "Wide", "ajax", "amid", "anti",
                "apud", "atop", "bout", "come", "dahn", "doon", "down", "From", "Gain", "Into",
                "Like", "Near", "Nigh", "Offa", "Onto", "Outa", "Over", "Past", "Post", "Save",
                "Than", "Thro", "Thru", "Till", "Unto", "Upon", "Vice", "Whiz", "With", "ahem",
                "ahoy", "alas", "amen", "bang", "blah", "ciao", "Egad", "Fore", "Gosh", "Hell",
                "Hist", "Hiya", "Hmmm", "Hmph", "Jeez", "Meow", "Nome", "Oops", "Ouch", "Phew",
                "Poof", "Pooh", "Pugh", "Shoo", "Urgh", "Waly", "Wham", "Whoa", "Yuck",
            ],
            index: 0,
        }
    }
}

#[derive(Debug)]
pub struct NamedStreamId {
    names: HashMap<String, usize>,
}
impl NamedStreamId {
    pub fn new() -> Self {
        Self {
            names: HashMap::new(),
        }
    }
    pub fn id(&mut self, id: impl AsRef<str>) -> String {
        let id = id.as_ref().to_string();
        match self.names.get_mut(&id) {
            None => {
                let new_id = format!("{}0", id);
                self.names.insert(id, 0);
                new_id
            }
            Some(index) => {
                *index += 1;
                format!("{}{}", id, index)
            }
        }
    }
}

#[test]
fn test_named_id() {
    let mut id = NamedStreamId::new();
    assert_eq!(id.id("video"), "video0".to_string());
    assert_eq!(id.id("video"), "video1".to_string());
    assert_eq!(id.id("background"), "background0".to_string());
    assert_eq!(id.id("background"), "background1".to_string());
    assert_eq!(id.id("video"), "video2".to_string());
}
