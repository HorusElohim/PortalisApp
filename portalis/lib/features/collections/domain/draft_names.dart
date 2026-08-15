import 'dart:math';

/// Names a new collection starts with, so it never begins as "Untitled".
///
/// A blank field asks a question before the person has seen what they are
/// naming, and "Untitled collection" is what four of them end up called. A
/// name that is already there is a suggestion rather than a demand: keep it,
/// or select it and type over it — the field opens focused for exactly that.
///
/// Deliberately playful and deliberately generic. None of these describes any
/// particular content, so none of them is ever *nearly* right in a way that
/// would tempt somebody to leave a wrong name on the wrong thing.
const draftNames = <String>[
  'Midnight Cargo',
  'Paper Lanterns',
  'Tuesday Haul',
  'Blue Hour',
  'Quiet Freight',
  'Salt and Static',
  'The Long Way',
  'Warm Static',
  'Corner Shop',
  'Slow Post',
  'Amber Signal',
  'Night Ferry',
  'Loose Change',
  'The Good Stuff',
  'Attic Boxes',
  'Low Tide',
  'Second Breakfast',
  'Paper Trail',
  'Small Hours',
  'Copper Wire',
  'Weekend Cargo',
  'Field Notes',
  'The Back Room',
  'Loose Ends',
  'Open Window',
  'Green Room',
  'Spare Parts',
  'Rainy Sunday',
  'Handful of Sand',
  'The Slow Lane',
  'Borrowed Light',
  'Cold Brew',
  'Desk Drawer',
  'Gone Fishing',
  'Half a Map',
  'Iron Kettle',
  'Jam Jar',
  'Kite String',
  'Left Luggage',
  'Marble Run',
  'North Pier',
  'Old Radio',
  'Pocket Lint',
  'Quiet Carriage',
  'Radio Silence',
  'Sunday Best',
  'Tin Roof',
  'Under the Stairs',
  'Velvet Rope',
  'Wandering Signal',
];

/// One of [draftNames], chosen at random.
///
/// Random rather than sequential: numbering them would make two collections
/// look related when the only thing they share is the order they were made in.
String randomDraftName([Random? random]) =>
    draftNames[(random ?? Random()).nextInt(draftNames.length)];
