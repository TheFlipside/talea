/** Best-effort default currency from the user's system locale. */

// There is no standard locale→currency API, so map the common regions and fall
// back to USD. Eurozone members map to EUR.
const REGION_TO_CURRENCY: Record<string, string> = {
  US: 'USD',
  GB: 'GBP',
  CH: 'CHF',
  JP: 'JPY',
  CN: 'CNY',
  CA: 'CAD',
  AU: 'AUD',
  NZ: 'NZD',
  SE: 'SEK',
  NO: 'NOK',
  DK: 'DKK',
  PL: 'PLN',
  CZ: 'CZK',
  HU: 'HUF',
  IN: 'INR',
  BR: 'BRL',
  MX: 'MXN',
  ZA: 'ZAR',
  KR: 'KRW',
  RU: 'RUB',
  TR: 'TRY',
  // Eurozone
  DE: 'EUR',
  FR: 'EUR',
  ES: 'EUR',
  IT: 'EUR',
  NL: 'EUR',
  BE: 'EUR',
  AT: 'EUR',
  IE: 'EUR',
  PT: 'EUR',
  FI: 'EUR',
  GR: 'EUR',
  SK: 'EUR',
  SI: 'EUR',
  EE: 'EUR',
  LV: 'EUR',
  LT: 'EUR',
  LU: 'EUR',
};

const FALLBACK = 'USD';

/** The default currency code for new accounts, derived from the system locale. */
export function defaultCurrency(): string {
  try {
    const locale = new Intl.Locale(navigator.language);
    const region = locale.maximize().region;
    if (region && REGION_TO_CURRENCY[region]) {
      return REGION_TO_CURRENCY[region];
    }
  } catch {
    // Fall through to the default.
  }
  return FALLBACK;
}
