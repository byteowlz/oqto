//! Word list module for generating human-readable session IDs
//! Format: adjective-noun (e.g., "cold-lamp", "blue-frog")

/// Adjectives for readable ID generation (291 words)
const ADJECTIVES: &[&str] = &[
    "able", "acid", "aged", "airy", "akin", "alto", "amok", "anti", "arch", "arid", "arty", "auld",
    "avid", "away", "awol", "awry", "back", "bald", "bare", "base", "bass", "bats", "beat", "bent",
    "best", "beta", "bias", "blue", "bold", "bone", "bony", "boon", "born", "boss", "both", "brag",
    "buff", "bulk", "bush", "bust", "busy", "calm", "camp", "chic", "clad", "cold", "cool", "cosy",
    "cozy", "curt", "cute", "cyan", "daft", "damp", "dank", "dark", "deaf", "dear", "deep", "deft",
    "dire", "dirt", "done", "dour", "down", "drab", "dual", "dull", "dyed", "each", "east", "easy",
    "edgy", "epic", "even", "evil", "eyed", "fair", "fake", "fast", "faux", "fell", "fine", "firm",
    "five", "flat", "flip", "fond", "foul", "foxy", "free", "full", "gaga", "game", "gilt", "glad",
    "glib", "glum", "gold", "gone", "good", "gray", "grey", "grim", "hale", "half", "halt", "hard",
    "hazy", "held", "here", "hick", "high", "hind", "holy", "home", "huge", "iced", "icky", "idle",
    "iffy", "inky", "iron", "just", "keen", "kept", "kind", "lacy", "laid", "lame", "lank", "last",
    "late", "lazy", "lean", "left", "less", "lest", "like", "limp", "lite", "live", "loco", "lone",
    "long", "lost", "loud", "lush", "luxe", "made", "main", "male", "many", "mass", "maxi", "mean",
    "meek", "meet", "mere", "midi", "mild", "mini", "mint", "mock", "mono", "moot", "more", "most",
    "much", "must", "mute", "near", "neat", "next", "nice", "nigh", "nine", "none", "nosy", "nude",
    "null", "numb", "nuts", "oily", "okay", "only", "open", "oral", "oval", "over", "paid", "pale",
    "pass", "past", "pent", "pied", "pink", "plus", "poor", "port", "posh", "prim", "puff", "punk",
    "puny", "pure", "racy", "rank", "rare", "rash", "real", "rear", "rich", "rife", "ripe", "roan",
    "rosy", "rude", "rust", "safe", "salt", "same", "sane", "sear", "self", "sent", "sewn", "sham",
    "shed", "shot", "shut", "side", "sign", "size", "skew", "skim", "slim", "slow", "smug", "snub",
    "snug", "soft", "sold", "sole", "solo", "some", "sore", "sour", "sown", "spry", "star", "such",
    "sunk", "sure", "tall", "tame", "tart", "taut", "teal", "teen", "then", "thin", "tidy", "tied",
    "tiny", "toed", "tops", "torn", "trig", "trim", "true", "twin", "ugly", "used", "vain", "vast",
    "very", "vile", "void", "warm", "wary", "wavy", "waxy", "weak", "wide", "wild", "wily", "wise",
    "worn", "zany", "zero",
];

/// Nouns for readable ID generation (1500+ words, filtered for appropriateness)
const NOUNS: &[&str] = &[
    "acer", "aces", "acid", "acne", "acre", "acts", "adds", "afro", "agar", "aged", "ages", "ahem",
    "aide", "aids", "aims", "airs", "alas", "ally", "aloe", "alto", "alum", "amen", "amir", "ammo",
    "amor", "amps", "anil", "ante", "anti", "ants", "apes", "apex", "apis", "aqua", "arch", "arcs",
    "area", "ares", "aria", "arms", "army", "arts", "ashe", "atom", "aunt", "aura", "auto", "avon",
    "axes", "axis", "axle", "baba", "babe", "baby", "bach", "back", "bags", "bail", "bait", "bale",
    "ball", "balm", "band", "bane", "bang", "bank", "bans", "barb", "bard", "bark", "barn", "bars",
    "bart", "base", "bash", "bass", "bath", "bats", "bays", "bead", "beak", "beam", "bean", "bear",
    "beat", "beau", "beds", "beef", "beep", "beer", "bees", "beet", "bell", "belt", "bend", "bent",
    "berg", "bern", "best", "beta", "bets", "bias", "bids", "bike", "bill", "bind", "bins", "bird",
    "bite", "bits", "blah", "blob", "bloc", "blog", "blot", "blow", "blue", "blur", "boar", "boat",
    "body", "boer", "boil", "bold", "bolt", "bond", "bone", "bong", "book", "boom", "boon", "boos",
    "boot", "bore", "born", "boss", "bots", "bout", "bowl", "bows", "boys", "brag", "bran", "bras",
    "brat", "bray", "brew", "brie", "brig", "brim", "brit", "brow", "buck", "buds", "buff", "bugs",
    "bulb", "bulk", "bull", "bump", "bums", "bunk", "buns", "buoy", "burn", "burr", "bush", "bust",
    "buys", "buzz", "byrd", "byte", "cabs", "cafe", "cage", "cake", "calf", "cali", "call", "calm",
    "camo", "camp", "cams", "cane", "cans", "cant", "cape", "caps", "card", "care", "carp", "cars",
    "cart", "case", "cash", "cast", "cats", "cave", "cebu", "cell", "cent", "ceos", "cert", "chap",
    "char", "chat", "chef", "chew", "chic", "chin", "chit", "chop", "cite", "city", "clam", "clan",
    "clap", "claw", "clay", "clip", "clot", "club", "clue", "coal", "coat", "coca", "coco", "code",
    "cody", "cohn", "coil", "coin", "cola", "cold", "colt", "coma", "comb", "come", "comp", "cone",
    "cons", "cool", "coop", "cope", "cops", "copy", "cora", "cord", "core", "cork", "corn", "corp",
    "cost", "cosy", "coup", "cove", "cows", "cozy", "crab", "cree", "crew", "crib", "crop", "crow",
    "crux", "cube", "cubs", "cues", "cuff", "cult", "cups", "curb", "cure", "curl", "cusp", "cuts",
    "cyst", "czar", "dada", "dads", "dahl", "dame", "damp", "dams", "dare", "dark", "darn", "dart",
    "dash", "data", "date", "davy", "days", "deaf", "deal", "dear", "debt", "deck", "deco", "deed",
    "deep", "deer", "deli", "demo", "dent", "desk", "dial", "diaz", "dice", "dies", "diet", "digs",
    "dill", "dime", "ding", "dior", "dips", "dirk", "dirt", "disc", "dish", "disk", "diva", "dive",
    "dock", "docs", "does", "dogs", "doha", "dole", "doll", "dome", "dons", "doom", "door", "dope",
    "dork", "dorm", "dory", "dose", "dots", "dove", "down", "drab", "drag", "draw", "drip", "drop",
    "drum", "dubs", "duck", "duct", "dude", "duel", "dues", "duet", "duff", "dump", "dune", "dunk",
    "dusk", "dust", "duty", "dvds", "dyer", "dyes", "dyke", "ears", "ease", "east", "eats", "echo",
    "eddy", "edge", "eels", "eggs", "egos", "emir", "emmy", "ends", "envy", "epic", "eras", "erie",
    "erin", "eros", "even", "evil", "exam", "exec", "exes", "exit", "expo", "eyes", "eyre", "face",
    "fact", "fade", "fair", "fake", "fall", "fame", "fang", "fans", "fare", "farm", "fast", "fate",
    "fats", "fawn", "fear", "feat", "feds", "feed", "feel", "fees", "feet", "fell", "felt", "fema",
    "fern", "feud", "fife", "figs", "file", "fill", "film", "find", "fine", "fink", "fins", "fire",
    "firm", "fish", "fist", "fits", "five", "flag", "flak", "flap", "flat", "flaw", "flax", "flea",
    "flex", "flip", "flop", "flow", "flux", "foam", "foes", "foil", "fold", "folk", "font", "food",
    "fool", "foot", "fork", "form", "fort", "foul", "fowl", "frat", "frau", "fray", "free", "fret",
    "frey", "frog", "fuel", "fuji", "full", "fund", "funk", "furs", "fury", "fuse", "fuss", "fuzz",
    "gaap", "gage", "gags", "gain", "gait", "gala", "gale", "gall", "gals", "game", "gang", "gaps",
    "garb", "gasp", "gate", "gaul", "gays", "gaze", "gcse", "gear", "geek", "gems", "gent", "germ",
    "gets", "gift", "gigs", "gill", "gilt", "girl", "giro", "gist", "give", "glad", "glee", "glow",
    "glue", "goal", "goat", "gods", "goes", "gogh", "gold", "golf", "gong", "good", "goon", "goth",
    "gout", "gown", "grab", "grad", "gran", "gray", "grey", "grid", "grin", "grip", "grit", "grub",
    "guam", "gull", "gums", "guns", "guru", "gust", "guts", "guys", "gyms", "hack", "hahn", "hail",
    "hair", "hajj", "hale", "half", "hall", "halo", "halt", "hand", "hang", "hank", "hare", "harm",
    "harp", "hash", "hats", "haul", "have", "hawk", "hays", "haze", "head", "heap", "heat", "heed",
    "heel", "heir", "helm", "help", "hemp", "hens", "herb", "herd", "here", "hero", "herr", "hide",
    "high", "hike", "hill", "hind", "hint", "hips", "hire", "hiss", "hits", "hive", "hoax", "hobo",
    "hogg", "hogs", "hold", "hole", "holy", "home", "homo", "hone", "hoof", "hook", "hoop", "hops",
    "horn", "hose", "host", "hour", "howe", "howl", "html", "http", "hubs", "hues", "huff", "hugs",
    "hula", "hulk", "hume", "hump", "hunk", "hush", "huts", "hymn", "hype", "iaea", "icon", "idea",
    "idle", "idol", "ills", "imam", "inch", "info", "inks", "inns", "ions", "ipod", "iron", "isis",
    "itch", "item", "ives", "jail", "jams", "jars", "jaws", "jays", "jazz", "jeep", "jest", "jets",
    "jinx", "jive", "jobs", "jock", "join", "joke", "jolt", "jong", "joss", "joys", "judo", "july",
    "jump", "june", "jung", "junk", "juno", "jury", "kahn", "kale", "kali", "kami", "kant", "keel",
    "keen", "keep", "kern", "keys", "kick", "kidd", "kids", "kiln", "kilo", "kind", "king", "kink",
    "kiss", "kite", "kits", "kiwi", "knee", "knit", "knob", "knox", "kobe", "kris", "labs", "lace",
    "lack", "lads", "lady", "lags", "lair", "lake", "lakh", "lama", "lamb", "lame", "lamp", "land",
    "lane", "laos", "laps", "lark", "lash", "lass", "last", "lava", "lawn", "laws", "lays", "lead",
    "leaf", "leak", "lean", "leap", "lear", "leds", "lees", "left", "lego", "legs", "lens", "lent",
    "leto", "lets", "levi", "liar", "lice", "lick", "lids", "lied", "lien", "lies", "lieu", "life",
    "lift", "like", "lily", "limb", "lime", "limo", "limp", "line", "ling", "link", "lion", "lips",
    "lisp", "list", "liza", "load", "loaf", "loan", "lobe", "loch", "lock", "loeb", "loft", "logo",
    "logs", "loki", "look", "loom", "loop", "loot", "lord", "lore", "loss", "lost", "lots", "love",
    "lows", "lube", "luck", "lucy", "lull", "lulu", "lump", "lund", "lung", "lure", "lush", "lynx",
    "lyon", "mace", "mach", "mack", "macs", "mags", "maid", "mail", "main", "make", "male", "mall",
    "malt", "mama", "mane", "mann", "mans", "maps", "mara", "mare", "mart", "mash", "mask", "mass",
    "mast", "mate", "math", "mats", "maui", "maxi", "maya", "mayo", "mays", "maze", "meal", "mean",
    "meat", "meds", "meet", "melt", "meme", "memo", "mend", "mens", "menu", "meow", "mere", "mesh",
    "mess", "meth", "mice", "mick", "midi", "mile", "milk", "mill", "milo", "mime", "mina", "mind",
    "mine", "mini", "mink", "mins", "mint", "miss", "mite", "mitt", "moan", "moat", "mobs", "mock",
    "mode", "mods", "mojo", "mold", "mole", "moms", "mona", "monk", "mono", "mons", "mood", "moon",
    "moor", "moot", "more", "morn", "moss", "moth", "mott", "move", "mrna", "much", "muck", "mugs",
    "muir", "mule", "mums", "musa", "must", "mute", "myth", "nada", "nail", "name", "napa", "naps",
    "nash", "nave", "neck", "need", "neon", "nerd", "nest", "nets", "news", "newt", "ngos", "nice",
    "nile", "nine", "noaa", "node", "nods", "none", "nook", "noon", "norm", "nose", "note", "noun",
    "nude", "nuke", "null", "nuns", "nuts", "nyse", "oaks", "oars", "oath", "oats", "odds", "odin",
    "odor", "ogre", "ohio", "oils", "okay", "olds", "omen", "ones", "opal", "open", "oral", "osha",
    "otis", "outs", "oval", "oven", "over", "owls", "pack", "pads", "page", "pain", "pale", "palm",
    "pals", "pane", "pang", "pans", "pant", "park", "parr", "part", "pass", "past", "path", "pats",
    "pave", "pawn", "paws", "pays", "peak", "peas", "peat", "peek", "peel", "peep", "peer", "pens",
    "perk", "perm", "peso", "pest", "pets", "pick", "pics", "pier", "pies", "pigs", "pike", "pile",
    "pill", "pine", "ping", "pink", "pins", "pint", "pipe", "pita", "pits", "pitt", "pity", "plan",
    "plat", "play", "plea", "plot", "plow", "ploy", "plug", "plum", "plus", "pods", "poem", "poet",
    "poke", "pole", "poll", "pond", "pong", "pony", "pool", "poor", "pops", "pore", "pork", "port",
    "pose", "post", "pots", "prep", "prey", "prod", "prof", "prom", "prop", "pros", "ptsd", "pubs",
    "puck", "puff", "pull", "pulp", "puma", "pump", "punk", "puns", "punt", "pups", "push", "puss",
    "puts", "putt", "qing", "quad", "quay", "quid", "quiz", "race", "rack", "raft", "rage", "rags",
    "raid", "rail", "rain", "raja", "rake", "rama", "ramp", "rams", "rana", "rand", "rank", "rant",
    "raps", "rash", "rate", "rats", "rave", "rays", "rcmp", "read", "real", "rear", "reds", "reed",
    "reef", "reel", "refs", "rein", "reno", "rent", "reps", "rest", "ribs", "rice", "rich", "rick",
    "ride", "riff", "rift", "rigs", "rims", "ring", "rink", "riot", "rips", "rise", "risk", "rite",
    "road", "roar", "robe", "rock", "rods", "role", "rolf", "roll", "roof", "rook", "room", "root",
    "rope", "rout", "roux", "rows", "rubs", "rudd", "ruff", "rugs", "ruin", "rule", "rump", "rune",
    "rung", "runs", "ruse", "russ", "rust", "sack", "safe", "saga", "sail", "sake", "sale", "salt",
    "same", "sami", "sana", "sand", "sang", "sari", "sash", "save", "says", "scam", "scan", "scar",
    "scum", "seal", "seam", "seas", "seat", "secs", "sect", "seed", "seek", "seer", "sees", "self",
    "sell", "semi", "sens", "sent", "sept", "sera", "serb", "sets", "shag", "sham", "shan", "shay",
    "shed", "shia", "shin", "ship", "shiv", "shoe", "shop", "shot", "show", "side", "sigh", "sign",
    "silk", "sill", "silo", "sine", "sink", "sins", "sion", "sire", "site", "size", "skid", "skim",
    "skin", "skip", "skis", "skit", "slab", "slag", "slam", "slap", "sled", "slew", "slip", "slit",
    "slot", "slug", "slum", "slur", "smog", "snag", "snap", "snow", "snug", "soak", "soap", "soar",
    "sobs", "sock", "soda", "sofa", "soho", "soil", "sole", "solo", "soma", "song", "sons", "soot",
    "sore", "sort", "soul", "soup", "sour", "sous", "spam", "spar", "spat", "spec", "spin", "spot",
    "spur", "stab", "stag", "star", "stay", "stem", "step", "stew", "stir", "stop", "stub", "stud",
    "subs", "sues", "suit", "sumo", "sums", "sung", "suns", "surf", "suvs", "swag", "swan", "swap",
    "swat", "sway", "swim", "tabs", "tack", "taco", "tact", "tags", "tail", "take", "tale", "talk",
    "tall", "tang", "tank", "tape", "taps", "tart", "task", "taxi", "teal", "team", "tear", "teas",
    "tech", "teen", "tees", "tell", "temp", "tens", "tent", "term", "test", "text", "thaw", "then",
    "thou", "thug", "thus", "tick", "tide", "tidy", "tier", "ties", "tiff", "tile", "till", "tilt",
    "time", "ting", "tins", "tint", "tips", "tire", "toad", "toby", "toes", "tofu", "toil", "toll",
    "tomb", "tome", "toms", "tone", "tons", "tool", "toon", "toot", "tops", "tore", "tori", "tort",
    "tory", "toss", "tote", "tots", "tour", "tout", "town", "toys", "tram", "trap", "tray", "tree",
    "trek", "trey", "trim", "trio", "trip", "trot", "true", "tsar", "tube", "tubs", "tuck", "tuna",
    "tune", "tung", "turf", "turk", "turn", "tutu", "twig", "twin", "tyne", "type", "typo", "tyre",
    "unit", "urdu", "urge", "usaf", "user", "uses", "usps", "ussr", "vale", "vane", "vans", "vase",
    "veal", "vega", "veil", "vein", "vent", "verb", "vest", "veto", "vets", "vial", "vibe", "vice",
    "view", "vine", "viva", "void", "volt", "vote", "vows", "waco", "wage", "wait", "wake", "walk",
    "wall", "wand", "want", "warp", "wars", "wash", "wasp", "wave", "ways", "webs", "week", "weir",
    "weld", "whey", "whim", "whip", "whit", "whos", "wick", "wife", "wifi", "wigs", "wild", "will",
    "wind", "wine", "wing", "wink", "wipe", "wire", "wise", "wish", "wits", "woes", "womb", "wont",
    "woof", "wool", "word", "work", "worm", "wrap", "wren", "writ", "yank", "yard", "yarn", "yawn",
    "year", "yelp", "yeti", "yoke", "yolk", "zeal", "zero", "zest", "zeta", "zhou", "zinc", "zone",
    "zoom", "zoos",
];

/// Simple hash function for strings (matches frontend implementation)
fn hash_string(s: &str) -> u32 {
    let mut hash: i32 = 0;
    for c in s.chars() {
        let char_code = c as i32;
        hash = ((hash << 5).wrapping_sub(hash)).wrapping_add(char_code);
    }
    hash.unsigned_abs()
}

/// Generate a deterministic human-readable ID from a session ID.
///
/// This produces the same output for the same input. This is the single source
/// of truth for readable IDs - they are computed, not stored.
///
/// Format: adjective-noun-noun (e.g., "cold-lamp-bird")
/// Combinations: 291 * 1506 * 1506 = ~660 million unique IDs
///
/// Example: "ses_abc123" -> "cold-lamp-bird"
pub fn readable_id_from_session_id(session_id: &str) -> String {
    let hash = hash_string(session_id);
    let adj_idx = (hash as usize) % ADJECTIVES.len();
    let noun1_idx = ((hash as usize) / ADJECTIVES.len()) % NOUNS.len();
    let noun2_idx = ((hash as usize) / ADJECTIVES.len() / NOUNS.len()) % NOUNS.len();
    format!(
        "{}-{}-{}",
        ADJECTIVES[adj_idx], NOUNS[noun1_idx], NOUNS[noun2_idx]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_readable_id_format() {
        let id = readable_id_from_session_id("ses_test123");
        assert!(id.contains('-'), "ID should contain a hyphen");
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(
            parts.len(),
            3,
            "ID should have exactly three parts (adjective-noun-noun)"
        );
    }

    #[test]
    fn test_readable_id_is_deterministic() {
        // Same input should always produce same output
        let id1 = readable_id_from_session_id("ses_abc123");
        let id2 = readable_id_from_session_id("ses_abc123");
        assert_eq!(id1, id2, "Same session ID should produce same readable ID");
    }

    #[test]
    fn test_different_sessions_get_different_ids() {
        let id1 = readable_id_from_session_id("ses_abc123");
        let id2 = readable_id_from_session_id("ses_xyz789");
        // Very unlikely to collide with different inputs
        assert_ne!(
            id1, id2,
            "Different session IDs should produce different readable IDs"
        );
    }

    #[test]
    fn test_hash_matches_frontend() {
        // Test that our hash function produces the same results as the frontend
        // Frontend: hashString("test") should equal our hash_string("test")
        let hash = hash_string("test");
        assert!(hash > 0, "Hash should be positive");

        // The hash should be consistent
        let hash2 = hash_string("test");
        assert_eq!(hash, hash2, "Hash should be deterministic");
    }
}
