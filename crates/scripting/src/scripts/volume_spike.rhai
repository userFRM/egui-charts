// Volume against its average, marked where it passes a multiple of it.
input length = length("Length", 20, 1, 500);
input factor = float("Factor", 2.0, 1.0, 10.0);
let average = sma(volume, length);
plot("Volume", volume, style = "columns");
plot("Average", average);
plot("Spike", if volume > average * factor { volume } else { na }, style = "circles");
