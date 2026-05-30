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

/**
 * Categories seeded on first run. Labels are i18n keys (localized at seed).
 *
 * There is deliberately no "Other" here: uncategorized expenses are *themselves*
 * the "Other" bucket on the stats screen (see `stats.other`), so seeding a real
 * "Other" category would create a confusing duplicate slice.
 */
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
];
