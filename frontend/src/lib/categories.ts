/** Category UI helpers: icon rendering, the emoji picker set, and the
 * first-run default categories. */

import type { CategoryIcon } from '../api/types';

/** Display text for a category icon (the emoji, or a tag fallback for presets). */
export function categoryIconText(icon: CategoryIcon): string {
  return 'emoji' in icon ? icon.emoji : '🏷️';
}

/** Curated emojis offered in the category icon picker. */
export const CATEGORY_EMOJIS: string[] = [
  '🛒', '🍽️', '🍎', '🍞', '☕', '🍺', '🍕', '🍔',
  '🏠', '🔑', '💡', '🔥', '💧', '🛋️', '🧹', '🔧',
  '🚌', '🚗', '⛽', '🚆', '✈️', '🚲', '🛵', '🅿️',
  '🏥', '💊', '🩺', '🦷', '🧘', '🏋️', '💉', '🩹',
  '🎬', '🎮', '🎵', '📺', '🎟️', '📚', '🎨', '⚽',
  '🛍️', '👕', '👟', '💄', '💇', '✂️', '🎁', '💐',
  '💰', '🏦', '💳', '📈', '💵', '🪙', '🧾', '💼',
  '📱', '💻', '📶', '👶', '🐕', '🐈', '🎓', '🏷️',
];

/** Categories seeded on first run. Labels are i18n keys (localized at seed). */
export const DEFAULT_CATEGORIES: { labelKey: string; emoji: string }[] = [
  { labelKey: 'defaultCategories.groceries', emoji: '🛒' },
  { labelKey: 'defaultCategories.dining', emoji: '🍽️' },
  { labelKey: 'defaultCategories.transport', emoji: '🚌' },
  { labelKey: 'defaultCategories.housing', emoji: '🏠' },
  { labelKey: 'defaultCategories.utilities', emoji: '💡' },
  { labelKey: 'defaultCategories.health', emoji: '🏥' },
  { labelKey: 'defaultCategories.entertainment', emoji: '🎬' },
  { labelKey: 'defaultCategories.shopping', emoji: '🛍️' },
  { labelKey: 'defaultCategories.salary', emoji: '💰' },
  { labelKey: 'defaultCategories.savings', emoji: '🏦' },
  { labelKey: 'defaultCategories.gifts', emoji: '🎁' },
  { labelKey: 'defaultCategories.other', emoji: '🏷️' },
];
