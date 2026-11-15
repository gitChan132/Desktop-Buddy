import amberSpritesheet from "./assets/frog/amber_spritesheet.png";
import blueSpritesheet from "./assets/frog/blue_spritesheet.png";
import greenSpritesheet from "./assets/frog/green_spritesheet.png";
import pinkSpritesheet from "./assets/frog/pink_spritesheet.png";

/** Keep in sync with `FrogColor` in `src-tauri/src/settings.rs`. */
export type FrogColor = "green" | "blue" | "pink" | "amber";

/** Cycle order for the pond's colour arrows. */
export const FROG_COLORS: FrogColor[] = ["green", "blue", "pink", "amber"];

export const FROG_COLOR_LABELS: Record<FrogColor, string> = {
  green: "Green",
  blue: "Blue",
  pink: "Pink",
  amber: "Amber",
};

export const SPRITESHEETS: Record<FrogColor, string> = {
  green: greenSpritesheet,
  blue: blueSpritesheet,
  pink: pinkSpritesheet,
  amber: amberSpritesheet,
};

/** Steps through the palette, wrapping at both ends. */
export const shiftFrogColor = (color: FrogColor, direction: 1 | -1): FrogColor => {
  const index = FROG_COLORS.indexOf(color);
  return FROG_COLORS[(index + direction + FROG_COLORS.length) % FROG_COLORS.length];
};
