//! Unix fortune cookie mini-feature used by public channel command &lt;prefix&gt;FORTUNE (default prefix `^`).
//!
//! This module provides a stateless fortune cookie system inspired by the classic Unix
//! `fortune` command. It contains a curated database of user‑provided fortunes: witty,
//! funny, thoughtful, and clean-each under 200 characters.
//! Humor, proverbs, haiku, limericks, and motivational messages.

use rand::Rng;
/// Curated collection of fortune cookies sourced from user‑provided list.
/// All entries under 200 characters for mesh network compatibility.
const FORTUNES: [&str; 400] = [
    "Fall seven times, stand up eight.",
    "I once ate a clock… it was time consuming.",
    "You can’t make an omelet without breaking eggs.",
    "Winter seclusion\nListening, that evening,\nRain in the mountain.",
    "Measure twice, cut once.",
    "In the cicada's cry\nNo sign can foretell\nHow soon it must die.",
    "Why was the math book sad? Too many problems.",
    "Silence is often the best answer.",
    "Parallel lines have so much in common… it’s a shame they’ll never meet.",
    "What do you call fake spaghetti? An impasta.",
    "A fellow who loved to eat pie\nWould stack them as high as the sky.\nHe gobbled them quick,\nUntil he felt sick,\nAnd sighed with a satisfied sigh.",
    "I told my computer I needed a break… it froze.",
    "The grass is always greener on the other side.",
    "Morning glory blooms\nSun warms the quiet garden\nBees hum lazily.",
    "Curiosity killed the cat, but satisfaction brought it back.",
    "How do cows stay up to date? They read the moos-paper.",
    "Autumn leaves scatter\nDancing in the chilly breeze\nColors fade to earth.",
    "In the cicada's cry\nNo sign can foretell\nHow soon it must die.",
    "Winter seclusion\nListening, that evening,\nRain in the mountain.",
    "The squeaky wheel gets the grease.",
    "When the student is ready, the teacher appears.",
    "Fall seven times, stand up eight.",
    "A farmer who lived by the sea\nKept chickens as happy as could be.\nOne morning at dawn,\nThey danced on the lawn,\nAnd clucked out a jubilant plea.",
    "Lonely path at dusk\nCrickets sing a gentle song\nMoon watches above.",
    "There was a young lady from Bright,\nWho traveled much faster than light.\nShe departed one day,\nIn a relative way,\nAnd returned on the previous night.",
    "Don’t count your chickens before they hatch.",
    "Why don’t scientists trust atoms? Because they make up everything.",
    "Winter seclusion\nListening, that evening,\nRain in the mountain.",
    "There once was a man from Peru\nWho dreamed he was eating his shoe.\nHe woke with a fright\nIn the middle of night\nAnd found that his dream had come true.",
    "Why was the math book sad? Too many problems.",
    "Curiosity killed the cat, but satisfaction brought it back.",
    "There was a young lady from Bright,\nWho traveled much faster than light.\nShe departed one day,\nIn a relative way,\nAnd returned on the previous night.",
    "You can’t make an omelet without breaking eggs.",
    "Strike while the iron is hot.",
    "Spring rain on soft ground\nEarth awakens quietly\nLife begins again.",
    "There once was a man from Peru\nWho dreamed he was eating his shoe.\nHe woke with a fright\nIn the middle of night\nAnd found that his dream had come true.",
    "Strike while the iron is hot.",
    "There was a young lady from Bright,\nWho traveled much faster than light.\nShe departed one day,\nIn a relative way,\nAnd returned on the previous night.",
    "Do not fear moving slowly, fear standing still.",
    "Curiosity killed the cat, but satisfaction brought it back.",
    "A student whose habits were slack\nWas warned of the work he should track.\nHe laughed with delight,\nPulled an all-nighter that night,\nThen slept through the test in the back.",
    "Over the wintry\nForest, winds howl in rage\nWith no leaves to blow.",
    "Winter seclusion\nListening, that evening,\nRain in the mountain.",
    "Over the wintry\nForest, winds howl in rage\nWith no leaves to blow.",
    "How do cows stay up to date? They read the moos-paper.",
    "Over the wintry\nForest, winds howl in rage\nWith no leaves to blow.",
    "Even the tallest oak was once a tiny acorn.",
    "There once was a baker named Lee\nWho made bread as tall as a tree.\nIt toppled one day,\nIn a doughy display,\nAnd flattened poor Lee to a pea.",
    "Why was the math book sad? Too many problems.",
    "Why did the golfer bring two pairs of pants? In case he got a hole in one.",
    "Why did the golfer bring two pairs of pants? In case he got a hole in one.",
    "A journey of a thousand miles begins with a single step.",
    "Curiosity killed the cat, but satisfaction brought it back.",
    "There was a young fellow named Flynn\nWho was known for his mischievous grin.\nHe once stole a pie,\nFrom the baker nearby,\nAnd was banned from the bakery within.",
    "A fellow who loved to eat pie\nWould stack them as high as the sky.\nHe gobbled them quick,\nUntil he felt sick,\nAnd sighed with a satisfied sigh.",
    "Winter seclusion\nListening, that evening,\nRain in the mountain.",
    "The obstacle is the path.",
    "Morning glory blooms\nSun warms the quiet garden\nBees hum lazily.",
    "There was a young lady from Bright,\nWho traveled much faster than light.\nShe departed one day,\nIn a relative way,\nAnd returned on the previous night.",
    "I told my computer I needed a break… it froze.",
    "A mouse in her room woke Miss Dowd\nShe was frightened, it must be allowed.\nSoon a happy thought hit her,\nTo scare off the critter,\nShe sat up in bed and meowed.",
    "Fall seven times, stand up eight.",
    "A journey of a thousand miles begins with a single step.",
    "Winter seclusion\nListening, that evening,\nRain in the mountain.",
    "A student whose habits were slack\nWas warned of the work he should track.\nHe laughed with delight,\nPulled an all-nighter that night,\nThen slept through the test in the back.",
    "No snowflake ever falls in the wrong place.",
    "I once ate a clock… it was time consuming.",
    "An old silent pond\nA frog jumps into the pond\nSplash! Silence again.",
    "The obstacle is the path.",
    "What you seek is seeking you.",
    "Strike while the iron is hot.",
    "Over the wintry\nForest, winds howl in rage\nWith no leaves to blow.",
    "There once was a cat on a chair\nWho licked at the cream with great flair.\nThe dog gave a bark,\nIt leapt with a spark,\nAnd spilled it all over the stair.",
    "What do you call fake spaghetti? An impasta.",
    "You can’t make an omelet without breaking eggs.",
    "Spring rain on soft ground\nEarth awakens quietly\nLife begins again.",
    "Over the wintry\nForest, winds howl in rage\nWith no leaves to blow.",
    "Don’t count your chickens before they hatch.",
    "Fall seven times, stand up eight.",
    "The obstacle is the path.",
    "Do not fear moving slowly, fear standing still.",
    "Curiosity killed the cat, but satisfaction brought it back.",
    "You can’t make an omelet without breaking eggs.",
    "No snowflake ever falls in the wrong place.",
    "There once was a cat on a chair\nWho licked at the cream with great flair.\nThe dog gave a bark,\nIt leapt with a spark,\nAnd spilled it all over the stair.",
    "I told my computer I needed a break… it froze.",
    "Autumn leaves scatter\nDancing in the chilly breeze\nColors fade to earth.",
    "The squeaky wheel gets the grease.",
    "An old silent pond\nA frog jumps into the pond\nSplash! Silence again.",
    "There once was a baker named Lee\nWho made bread as tall as a tree.\nIt toppled one day,\nIn a doughy display,\nAnd flattened poor Lee to a pea.",
    "Strike while the iron is hot.",
    "Strike while the iron is hot.",
    "Why don’t scientists trust atoms? Because they make up everything.",
    "Curiosity killed the cat, but satisfaction brought it back.",
    "There once was a man from Peru\nWho dreamed he was eating his shoe.\nHe woke with a fright\nIn the middle of night\nAnd found that his dream had come true.",
    "Do not fear moving slowly, fear standing still.",
    "Better late than never.",
    "What you seek is seeking you.",
    "Spring rain on soft ground\nEarth awakens quietly\nLife begins again.",
    "How do cows stay up to date? They read the moos-paper.",
    "Why did the golfer bring two pairs of pants? In case he got a hole in one.",
    "You can’t make an omelet without breaking eggs.",
    "Measure twice, cut once.",
    "Strike while the iron is hot.",
    "Spring rain on soft ground\nEarth awakens quietly\nLife begins again.",
    "How do cows stay up to date? They read the moos-paper.",
    "Why was the math book sad? Too many problems.",
    "The grass is always greener on the other side.",
    "No snowflake ever falls in the wrong place.",
    "I used to play piano by ear, now I use my hands.",
    "There once was a cat on a chair\nWho licked at the cream with great flair.\nThe dog gave a bark,\nIt leapt with a spark,\nAnd spilled it all over the stair.",
    "What do you call fake spaghetti? An impasta.",
    "Parallel lines have so much in common… it’s a shame they’ll never meet.",
    "There once was a baker named Lee\nWho made bread as tall as a tree.\nIt toppled one day,\nIn a doughy display,\nAnd flattened poor Lee to a pea.",
    "Lonely path at dusk\nCrickets sing a gentle song\nMoon watches above.",
    "I told my computer I needed a break… it froze.",
    "How do cows stay up to date? They read the moos-paper.",
    "What do you call fake spaghetti? An impasta.",
    "In the cicada's cry\nNo sign can foretell\nHow soon it must die.",
    "Morning glory blooms\nSun warms the quiet garden\nBees hum lazily.",
    "In the cicada's cry\nNo sign can foretell\nHow soon it must die.",
    "Better late than never.",
    "No snowflake ever falls in the wrong place.",
    "The grass is always greener on the other side.",
    "There was a young fellow named Flynn\nWho was known for his mischievous grin.\nHe once stole a pie,\nFrom the baker nearby,\nAnd was banned from the bakery within.",
    "Even the tallest oak was once a tiny acorn.",
    "There once was a baker named Lee\nWho made bread as tall as a tree.\nIt toppled one day,\nIn a doughy display,\nAnd flattened poor Lee to a pea.",
    "A fellow who loved to eat pie\nWould stack them as high as the sky.\nHe gobbled them quick,\nUntil he felt sick,\nAnd sighed with a satisfied sigh.",
    "The obstacle is the path.",
    "Strike while the iron is hot.",
    "When the student is ready, the teacher appears.",
    "There once was a cat on a chair\nWho licked at the cream with great flair.\nThe dog gave a bark,\nIt leapt with a spark,\nAnd spilled it all over the stair.",
    "I once ate a clock… it was time consuming.",
    "Better late than never.",
    "Autumn leaves scatter\nDancing in the chilly breeze\nColors fade to earth.",
    "Curiosity killed the cat, but satisfaction brought it back.",
    "Measure twice, cut once.",
    "Fall seven times, stand up eight.",
    "I used to play piano by ear, now I use my hands.",
    "The obstacle is the path.",
    "The squeaky wheel gets the grease.",
    "There once was a cat on a chair\nWho licked at the cream with great flair.\nThe dog gave a bark,\nIt leapt with a spark,\nAnd spilled it all over the stair.",
    "Why did the golfer bring two pairs of pants? In case he got a hole in one.",
    "In the cicada's cry\nNo sign can foretell\nHow soon it must die.",
    "The lamp once out\nCool stars enter\nThe window frame.",
    "Lonely path at dusk\nCrickets sing a gentle song\nMoon watches above.",
    "The squeaky wheel gets the grease.",
    "I told my computer I needed a break… it froze.",
    "There once was a cat on a chair\nWho licked at the cream with great flair.\nThe dog gave a bark,\nIt leapt with a spark,\nAnd spilled it all over the stair.",
    "Spring rain on soft ground\nEarth awakens quietly\nLife begins again.",
    "Spring rain on soft ground\nEarth awakens quietly\nLife begins again.",
    "A limerick's fun when it's short,\nWith a humorous kind of retort.\nIf it rambles too long,\nIt can lose all its song,\nAnd end in the dullest report.",
    "Strike while the iron is hot.",
    "Strike while the iron is hot.",
    "A limerick's fun when it's short,\nWith a humorous kind of retort.\nIf it rambles too long,\nIt can lose all its song,\nAnd end in the dullest report.",
    "I used to play piano by ear, now I use my hands.",
    "Better late than never.",
    "How do cows stay up to date? They read the moos-paper.",
    "There once was a baker named Lee\nWho made bread as tall as a tree.\nIt toppled one day,\nIn a doughy display,\nAnd flattened poor Lee to a pea.",
    "There once was a baker named Lee\nWho made bread as tall as a tree.\nIt toppled one day,\nIn a doughy display,\nAnd flattened poor Lee to a pea.",
    "How do cows stay up to date? They read the moos-paper.",
    "The squeaky wheel gets the grease.",
    "Silence is often the best answer.",
    "I used to play piano by ear, now I use my hands.",
    "What do you call fake spaghetti? An impasta.",
    "Better late than never.",
    "Over the wintry\nForest, winds howl in rage\nWith no leaves to blow.",
    "A journey of a thousand miles begins with a single step.",
    "A journey of a thousand miles begins with a single step.",
    "A watched pot never boils.",
    "The obstacle is the path.",
    "There once was a man from Peru\nWho dreamed he was eating his shoe.\nHe woke with a fright\nIn the middle of night\nAnd found that his dream had come true.",
    "When the student is ready, the teacher appears.",
    "What do you call fake spaghetti? An impasta.",
    "The squeaky wheel gets the grease.",
    "Parallel lines have so much in common… it’s a shame they’ll never meet.",
    "You can’t make an omelet without breaking eggs.",
    "Patience is bitter, but its fruit is sweet.",
    "A student whose habits were slack\nWas warned of the work he should track.\nHe laughed with delight,\nPulled an all-nighter that night,\nThen slept through the test in the back.",
    "A fellow who loved to eat pie\nWould stack them as high as the sky.\nHe gobbled them quick,\nUntil he felt sick,\nAnd sighed with a satisfied sigh.",
    "A limerick's fun when it's short,\nWith a humorous kind of retort.\nIf it rambles too long,\nIt can lose all its song,\nAnd end in the dullest report.",
    "What you seek is seeking you.",
    "Over the wintry\nForest, winds howl in rage\nWith no leaves to blow.",
    "What do you call fake spaghetti? An impasta.",
    "What you seek is seeking you.",
    "Even the tallest oak was once a tiny acorn.",
    "There was a young lady from Bright,\nWho traveled much faster than light.\nShe departed one day,\nIn a relative way,\nAnd returned on the previous night.",
    "A student whose habits were slack\nWas warned of the work he should track.\nHe laughed with delight,\nPulled an all-nighter that night,\nThen slept through the test in the back.",
    "No snowflake ever falls in the wrong place.",
    "I used to play piano by ear, now I use my hands.",
    "Silence is often the best answer.",
    "Why did the scarecrow win an award? He was outstanding in his field.",
    "How do cows stay up to date? They read the moos-paper.",
    "The lamp once out\nCool stars enter\nThe window frame.",
    "How do cows stay up to date? They read the moos-paper.",
    "There was a young lady from Bright,\nWho traveled much faster than light.\nShe departed one day,\nIn a relative way,\nAnd returned on the previous night.",
    "Morning glory blooms\nSun warms the quiet garden\nBees hum lazily.",
    "A journey of a thousand miles begins with a single step.",
    "Winter seclusion\nListening, that evening,\nRain in the mountain.",
    "Better late than never.",
    "A limerick's fun when it's short,\nWith a humorous kind of retort.\nIf it rambles too long,\nIt can lose all its song,\nAnd end in the dullest report.",
    "The grass is always greener on the other side.",
    "Over the wintry\nForest, winds howl in rage\nWith no leaves to blow.",
    "There was a young lady from Bright,\nWho traveled much faster than light.\nShe departed one day,\nIn a relative way,\nAnd returned on the previous night.",
    "I told my computer I needed a break… it froze.",
    "A student whose habits were slack\nWas warned of the work he should track.\nHe laughed with delight,\nPulled an all-nighter that night,\nThen slept through the test in the back.",
    "I told my computer I needed a break… it froze.",
    "Fall seven times, stand up eight.",
    "Spring rain on soft ground\nEarth awakens quietly\nLife begins again.",
    "A student whose habits were slack\nWas warned of the work he should track.\nHe laughed with delight,\nPulled an all-nighter that night,\nThen slept through the test in the back.",
    "No snowflake ever falls in the wrong place.",
    "Autumn leaves scatter\nDancing in the chilly breeze\nColors fade to earth.",
    "I used to play piano by ear, now I use my hands.",
    "Why did the scarecrow win an award? He was outstanding in his field.",
    "Silence is often the best answer.",
    "Why was the math book sad? Too many problems.",
    "What you seek is seeking you.",
    "Why did the golfer bring two pairs of pants? In case he got a hole in one.",
    "I used to play piano by ear, now I use my hands.",
    "Patience is bitter, but its fruit is sweet.",
    "How do cows stay up to date? They read the moos-paper.",
    "There was a young fellow named Flynn\nWho was known for his mischievous grin.\nHe once stole a pie,\nFrom the baker nearby,\nAnd was banned from the bakery within.",
    "There was a young fellow named Flynn\nWho was known for his mischievous grin.\nHe once stole a pie,\nFrom the baker nearby,\nAnd was banned from the bakery within.",
    "Do not fear moving slowly, fear standing still.",
    "Why did the golfer bring two pairs of pants? In case he got a hole in one.",
    "Winter seclusion\nListening, that evening,\nRain in the mountain.",
    "Snow falls silently\nCovering the earth in white\nAll is hushed tonight.",
    "A student whose habits were slack\nWas warned of the work he should track.\nHe laughed with delight,\nPulled an all-nighter that night,\nThen slept through the test in the back.",
    "A limerick's fun when it's short,\nWith a humorous kind of retort.\nIf it rambles too long,\nIt can lose all its song,\nAnd end in the dullest report.",
    "A limerick's fun when it's short,\nWith a humorous kind of retort.\nIf it rambles too long,\nIt can lose all its song,\nAnd end in the dullest report.",
    "Why did the scarecrow win an award? He was outstanding in his field.",
    "What you seek is seeking you.",
    "A mouse in her room woke Miss Dowd\nShe was frightened, it must be allowed.\nSoon a happy thought hit her,\nTo scare off the critter,\nShe sat up in bed and meowed.",
    "The squeaky wheel gets the grease.",
    "Do not fear moving slowly, fear standing still.",
    "Even the tallest oak was once a tiny acorn.",
    "I once ate a clock… it was time consuming.",
    "Fall seven times, stand up eight.",
    "In the cicada's cry\nNo sign can foretell\nHow soon it must die.",
    "A watched pot never boils.",
    "Measure twice, cut once.",
    "I once ate a clock… it was time consuming.",
    "Silence is often the best answer.",
    "Autumn leaves scatter\nDancing in the chilly breeze\nColors fade to earth.",
    "There once was a man from Peru\nWho dreamed he was eating his shoe.\nHe woke with a fright\nIn the middle of night\nAnd found that his dream had come true.",
    "Why don’t scientists trust atoms? Because they make up everything.",
    "You can’t make an omelet without breaking eggs.",
    "Even the tallest oak was once a tiny acorn.",
    "The lamp once out\nCool stars enter\nThe window frame.",
    "I used to play piano by ear, now I use my hands.",
    "Do not fear moving slowly, fear standing still.",
    "Fall seven times, stand up eight.",
    "Parallel lines have so much in common… it’s a shame they’ll never meet.",
    "Spring rain on soft ground\nEarth awakens quietly\nLife begins again.",
    "Fall seven times, stand up eight.",
    "The grass is always greener on the other side.",
    "Over the wintry\nForest, winds howl in rage\nWith no leaves to blow.",
    "No snowflake ever falls in the wrong place.",
    "Parallel lines have so much in common… it’s a shame they’ll never meet.",
    "The grass is always greener on the other side.",
    "Spring rain on soft ground\nEarth awakens quietly\nLife begins again.",
    "There was a young lady from Bright,\nWho traveled much faster than light.\nShe departed one day,\nIn a relative way,\nAnd returned on the previous night.",
    "Silence is often the best answer.",
    "Even the tallest oak was once a tiny acorn.",
    "Why did the scarecrow win an award? He was outstanding in his field.",
    "The grass is always greener on the other side.",
    "A mouse in her room woke Miss Dowd\nShe was frightened, it must be allowed.\nSoon a happy thought hit her,\nTo scare off the critter,\nShe sat up in bed and meowed.",
    "An old silent pond\nA frog jumps into the pond\nSplash! Silence again.",
    "Lonely path at dusk\nCrickets sing a gentle song\nMoon watches above.",
    "Why don’t scientists trust atoms? Because they make up everything.",
    "There once was a cat on a chair\nWho licked at the cream with great flair.\nThe dog gave a bark,\nIt leapt with a spark,\nAnd spilled it all over the stair.",
    "Parallel lines have so much in common… it’s a shame they’ll never meet.",
    "Patience is bitter, but its fruit is sweet.",
    "A watched pot never boils.",
    "Why did the golfer bring two pairs of pants? In case he got a hole in one.",
    "When the student is ready, the teacher appears.",
    "Better late than never.",
    "The obstacle is the path.",
    "Too many cooks spoil the broth.",
    "Better late than never.",
    "The squeaky wheel gets the grease.",
    "Fall seven times, stand up eight.",
    "The lamp once out\nCool stars enter\nThe window frame.",
    "What you seek is seeking you.",
    "Lonely path at dusk\nCrickets sing a gentle song\nMoon watches above.",
    "Lonely path at dusk\nCrickets sing a gentle song\nMoon watches above.",
    "Patience is bitter, but its fruit is sweet.",
    "In the cicada's cry\nNo sign can foretell\nHow soon it must die.",
    "Winter seclusion\nListening, that evening,\nRain in the mountain.",
    "Fall seven times, stand up eight.",
    "A limerick's fun when it's short,\nWith a humorous kind of retort.\nIf it rambles too long,\nIt can lose all its song,\nAnd end in the dullest report.",
    "There was a young lady from Bright,\nWho traveled much faster than light.\nShe departed one day,\nIn a relative way,\nAnd returned on the previous night.",
    "I used to play piano by ear, now I use my hands.",
    "A mouse in her room woke Miss Dowd\nShe was frightened, it must be allowed.\nSoon a happy thought hit her,\nTo scare off the critter,\nShe sat up in bed and meowed.",
    "There once was a cat on a chair\nWho licked at the cream with great flair.\nThe dog gave a bark,\nIt leapt with a spark,\nAnd spilled it all over the stair.",
    "Why did the golfer bring two pairs of pants? In case he got a hole in one.",
    "A mouse in her room woke Miss Dowd\nShe was frightened, it must be allowed.\nSoon a happy thought hit her,\nTo scare off the critter,\nShe sat up in bed and meowed.",
    "When the student is ready, the teacher appears.",
    "Snow falls silently\nCovering the earth in white\nAll is hushed tonight.",
    "Strike while the iron is hot.",
    "Why did the golfer bring two pairs of pants? In case he got a hole in one.",
    "You can’t make an omelet without breaking eggs.",
    "Spring rain on soft ground\nEarth awakens quietly\nLife begins again.",
    "Too many cooks spoil the broth.",
    "A journey of a thousand miles begins with a single step.",
    "The squeaky wheel gets the grease.",
    "Over the wintry\nForest, winds howl in rage\nWith no leaves to blow.",
    "What do you call fake spaghetti? An impasta.",
    "The squeaky wheel gets the grease.",
    "Morning glory blooms\nSun warms the quiet garden\nBees hum lazily.",
    "The squeaky wheel gets the grease.",
    "Why don’t scientists trust atoms? Because they make up everything.",
    "Even the tallest oak was once a tiny acorn.",
    "A mouse in her room woke Miss Dowd\nShe was frightened, it must be allowed.\nSoon a happy thought hit her,\nTo scare off the critter,\nShe sat up in bed and meowed.",
    "Too many cooks spoil the broth.",
    "What do you call fake spaghetti? An impasta.",
    "Even the tallest oak was once a tiny acorn.",
    "Do not fear moving slowly, fear standing still.",
    "Why was the math book sad? Too many problems.",
    "A mouse in her room woke Miss Dowd\nShe was frightened, it must be allowed.\nSoon a happy thought hit her,\nTo scare off the critter,\nShe sat up in bed and meowed.",
    "An old silent pond\nA frog jumps into the pond\nSplash! Silence again.",
    "What do you call fake spaghetti? An impasta.",
    "There once was a man from Peru\nWho dreamed he was eating his shoe.\nHe woke with a fright\nIn the middle of night\nAnd found that his dream had come true.",
    "Morning glory blooms\nSun warms the quiet garden\nBees hum lazily.",
    "The lamp once out\nCool stars enter\nThe window frame.",
    "In the cicada's cry\nNo sign can foretell\nHow soon it must die.",
    "I told my computer I needed a break… it froze.",
    "I once ate a clock… it was time consuming.",
    "There once was a baker named Lee\nWho made bread as tall as a tree.\nIt toppled one day,\nIn a doughy display,\nAnd flattened poor Lee to a pea.",
    "No snowflake ever falls in the wrong place.",
    "Winter seclusion\nListening, that evening,\nRain in the mountain.",
    "Fall seven times, stand up eight.",
    "Silence is often the best answer.",
    "Lonely path at dusk\nCrickets sing a gentle song\nMoon watches above.",
    "There once was a cat on a chair\nWho licked at the cream with great flair.\nThe dog gave a bark,\nIt leapt with a spark,\nAnd spilled it all over the stair.",
    "In the cicada's cry\nNo sign can foretell\nHow soon it must die.",
    "The squeaky wheel gets the grease.",
    "There once was a cat on a chair\nWho licked at the cream with great flair.\nThe dog gave a bark,\nIt leapt with a spark,\nAnd spilled it all over the stair.",
    "Don’t count your chickens before they hatch.",
    "What do you call fake spaghetti? An impasta.",
    "In the cicada's cry\nNo sign can foretell\nHow soon it must die.",
    "You can’t make an omelet without breaking eggs.",
    "A limerick's fun when it's short,\nWith a humorous kind of retort.\nIf it rambles too long,\nIt can lose all its song,\nAnd end in the dullest report.",
    "Even the tallest oak was once a tiny acorn.",
    "A farmer who lived by the sea\nKept chickens as happy as could be.\nOne morning at dawn,\nThey danced on the lawn,\nAnd clucked out a jubilant plea.",
    "Curiosity killed the cat, but satisfaction brought it back.",
    "Parallel lines have so much in common… it’s a shame they’ll never meet.",
    "You can’t make an omelet without breaking eggs.",
    "I once ate a clock… it was time consuming.",
    "I once ate a clock… it was time consuming.",
    "I once ate a clock… it was time consuming.",
    "Winter seclusion\nListening, that evening,\nRain in the mountain.",
    "What you seek is seeking you.",
    "An old silent pond\nA frog jumps into the pond\nSplash! Silence again.",
    "I used to play piano by ear, now I use my hands.",
    "Don’t count your chickens before they hatch.",
    "Do not fear moving slowly, fear standing still.",
    "Measure twice, cut once.",
    "Strike while the iron is hot.",
    "I used to play piano by ear, now I use my hands.",
    "A journey of a thousand miles begins with a single step.",
    "A watched pot never boils.",
    "Morning glory blooms\nSun warms the quiet garden\nBees hum lazily.",
    "In the cicada's cry\nNo sign can foretell\nHow soon it must die.",
    "There once was a cat on a chair\nWho licked at the cream with great flair.\nThe dog gave a bark,\nIt leapt with a spark,\nAnd spilled it all over the stair.",
    "A mouse in her room woke Miss Dowd\nShe was frightened, it must be allowed.\nSoon a happy thought hit her,\nTo scare off the critter,\nShe sat up in bed and meowed.",
    "A mouse in her room woke Miss Dowd\nShe was frightened, it must be allowed.\nSoon a happy thought hit her,\nTo scare off the critter,\nShe sat up in bed and meowed.",
    "A fellow who loved to eat pie\nWould stack them as high as the sky.\nHe gobbled them quick,\nUntil he felt sick,\nAnd sighed with a satisfied sigh.",
    "Strike while the iron is hot.",
    "Fall seven times, stand up eight.",
    "I used to play piano by ear, now I use my hands.",
    "A watched pot never boils.",
    "A journey of a thousand miles begins with a single step.",
    "You can’t make an omelet without breaking eggs.",
    "Why was the math book sad? Too many problems.",
    "Over the wintry\nForest, winds howl in rage\nWith no leaves to blow.",
    "Why did the golfer bring two pairs of pants? In case he got a hole in one.",
    "Why was the math book sad? Too many problems.",
    "There once was a baker named Lee\nWho made bread as tall as a tree.\nIt toppled one day,\nIn a doughy display,\nAnd flattened poor Lee to a pea.",
    "Why was the math book sad? Too many problems.",
    "There once was a man from Peru\nWho dreamed he was eating his shoe.\nHe woke with a fright\nIn the middle of night\nAnd found that his dream had come true.",
    "There was a young fellow named Flynn\nWho was known for his mischievous grin.\nHe once stole a pie,\nFrom the baker nearby,\nAnd was banned from the bakery within.",
    "What do you call fake spaghetti? An impasta.",
    "The lamp once out\nCool stars enter\nThe window frame.",
    "Fall seven times, stand up eight.",
    "Spring rain on soft ground\nEarth awakens quietly\nLife begins again.",
    "An old silent pond\nA frog jumps into the pond\nSplash! Silence again.",
    "A student whose habits were slack\nWas warned of the work he should track.\nHe laughed with delight,\nPulled an all-nighter that night,\nThen slept through the test in the back.",
    "An old silent pond\nA frog jumps into the pond\nSplash! Silence again.",
    "Snow falls silently\nCovering the earth in white\nAll is hushed tonight.",
    "I once ate a clock… it was time consuming.",
    "There once was a baker named Lee\nWho made bread as tall as a tree.\nIt toppled one day,\nIn a doughy display,\nAnd flattened poor Lee to a pea.",
    "Spring rain on soft ground\nEarth awakens quietly\nLife begins again.",
    "Over the wintry\nForest, winds howl in rage\nWith no leaves to blow.",
    "A journey of a thousand miles begins with a single step.",
    "In the cicada's cry\nNo sign can foretell\nHow soon it must die.",
    "Too many cooks spoil the broth.",
    "Winter seclusion\nListening, that evening,\nRain in the mountain.",
    "Why was the math book sad? Too many problems.",
    "Don’t count your chickens before they hatch.",
];

/// Pick a random fortune from the classic database.
///
/// Returns a reference to a static string containing a fortune cookie message.
/// All fortunes are guaranteed to be under 200 characters for mesh network compatibility.
///
/// # Examples
///
/// ```
/// use meshbbs::bbs::fortune::get_fortune;
///
/// let fortune = get_fortune();
/// assert!(!fortune.is_empty());
/// assert!(fortune.len() <= 200);
/// ```
///
/// # Thread Safety
///
/// This function is thread-safe and uses `rand::thread_rng()` for randomization.
/// Multiple calls from different threads will produce independent random results.
pub fn get_fortune() -> &'static str {
    let mut rng = rand::thread_rng();
    let idx = rng.gen_range(0..FORTUNES.len());
    FORTUNES[idx]
}

/// Get the total number of fortunes in the database.
///
/// This function is primarily useful for testing and diagnostics.
///
/// # Examples
///
/// ```
/// use meshbbs::bbs::fortune::fortune_count;
/// assert!(fortune_count() > 0);
/// ```
pub fn fortune_count() -> usize {
    FORTUNES.len()
}

/// Get the maximum length of any fortune in the database.
///
/// This function is useful for validation and ensuring all fortunes
/// meet the mesh network size constraints.
///
/// # Examples
///
/// ```
/// use meshbbs::bbs::fortune::max_fortune_length;
///
/// assert!(max_fortune_length() <= 200);
/// ```
pub fn max_fortune_length() -> usize {
    FORTUNES.iter().map(|f| f.len()).max().unwrap_or(0)
}

/// Expose the complete fortune database for diagnostics and analytics.
///
/// This returns the static slice so callers can compute aggregate statistics
/// without duplicating the data. Use sparingly to avoid leaking fortunes into
/// telemetry or logs.
pub fn fortunes() -> &'static [&'static str] {
    &FORTUNES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fortunes_nonzero_count() {
        assert!(FORTUNES.len() > 0);
    }

    #[test]
    fn all_fortunes_under_200_chars() {
        for (i, fortune) in FORTUNES.iter().enumerate() {
            assert!(
                fortune.len() <= 200,
                "Fortune {} is too long ({} chars): {}",
                i,
                fortune.len(),
                fortune
            );
        }
    }

    #[test]
    fn all_fortunes_non_empty() {
        for (i, fortune) in FORTUNES.iter().enumerate() {
            assert!(!fortune.is_empty(), "Fortune {} is empty", i);
        }
    }

    #[test]
    fn all_fortunes_contain_printable_chars() {
        for (i, fortune) in FORTUNES.iter().enumerate() {
            assert!(
                fortune
                    .chars()
                    .all(|c| !c.is_control() || c.is_ascii_whitespace()),
                "Fortune {} contains control characters: {}",
                i,
                fortune
            );
        }
    }

    #[test]
    fn fortune_returns_valid_response() {
        let fortune = get_fortune();
        assert!(!fortune.is_empty());
        assert!(fortune.len() <= 200);
        assert!(FORTUNES.contains(&fortune));
    }

    #[test]
    fn fortune_randomness_check() {
        // Run multiple times to ensure we get different results
        let mut results = std::collections::HashSet::new();
        for _ in 0..50 {
            results.insert(get_fortune());
        }
        // Should get at least 10 different fortunes in 50 tries
        assert!(
            results.len() >= 10,
            "Fortune randomness seems poor: only {} unique results",
            results.len()
        );
    }

    #[test]
    fn fortune_thread_safety_simulation() {
        // Simulate concurrent access by calling get_fortune many times rapidly
        let mut handles = vec![];

        for _ in 0..10 {
            let handle = std::thread::spawn(|| {
                let mut local_results = std::collections::HashSet::new();
                for _ in 0..20 {
                    local_results.insert(get_fortune());
                }
                local_results
            });
            handles.push(handle);
        }

        let mut all_results = std::collections::HashSet::new();
        for handle in handles {
            let thread_results = handle.join().unwrap();
            all_results.extend(thread_results);
        }

        // Should collect a good variety of fortunes across threads
        assert!(
            all_results.len() >= 15,
            "Concurrent access produced only {} unique fortunes",
            all_results.len()
        );
    }

    #[test]
    fn fortune_database_sanity() {
        // Basic sanity: counts match and max length is reasonable
        assert_eq!(fortune_count(), FORTUNES.len());
        assert!(max_fortune_length() <= 200);
    }

    #[test]
    fn fortune_count_matches_array() {
        assert_eq!(fortune_count(), FORTUNES.len());
    }

    #[test]
    fn max_fortune_length_validation() {
        let max_len = max_fortune_length();
        assert!(
            max_len <= 200,
            "Maximum fortune length {} exceeds 200 character limit",
            max_len
        );
        assert!(
            max_len > 0,
            "Maximum fortune length should be greater than 0"
        );

        // Verify it actually matches the longest fortune
        let actual_max = FORTUNES.iter().map(|f| f.len()).max().unwrap();
        assert_eq!(max_len, actual_max);
    }

    #[test]
    fn helper_functions_consistency() {
        // Ensure helper functions are consistent with the actual data
        assert_eq!(fortune_count(), FORTUNES.len());

        let calculated_max = FORTUNES.iter().map(|f| f.len()).max().unwrap_or(0);
        assert_eq!(max_fortune_length(), calculated_max);
    }
}
