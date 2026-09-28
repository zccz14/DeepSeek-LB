//! DeepSeek token pricing.
//!
//! Every rate is expressed in USD nanodollars per single token, so a model costs
//! `tokens * rate` nanos. DeepSeek publishes two tariffs: peak and off-peak, where
//! off-peak rates are exactly half of the peak rates. The applicable tariff depends
//! on the UTC time of the request (see [`is_peak`]).

use chrono::{Datelike, Timelike, Weekday};
use serde::Serialize;

use crate::payments::USD_NANOS;

/// Token usage of one completed request.
#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct Usage {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_tokens: i64,
}

impl Usage {
    pub fn merge(&mut self, other: Self) {
        self.input_tokens = self.input_tokens.max(other.input_tokens);
        self.output_tokens = self.output_tokens.max(other.output_tokens);
        self.cached_tokens = self.cached_tokens.max(other.cached_tokens);
    }
}

#[derive(Clone, Copy)]
struct TokenRates {
    cache_hit_usd_nanos: i64,
    cache_miss_usd_nanos: i64,
    output_usd_nanos: i64,
}

#[derive(Clone, Copy)]
struct TokenPricing {
    model: &'static str,
    peak: TokenRates,
    off_peak: TokenRates,
}

/// INVARIANT: These prices are release-owned. The computed cost is stored with each
/// audit event, so later price-table updates cannot rewrite history.
///
/// Sources: DeepSeek "Models & Pricing" (USD per 1M tokens, peak/off-peak).
/// `deepseek-v4-flash` and `deepseek-v4-flash-vision-exp` are retired model names that
/// DeepSeek still accepts and bills at the Flash price. `deepseek-v4-pro` keeps its own
/// published rates after DeepSeek decided to keep serving V4 Pro unchanged.
const TOKEN_PRICING: &[TokenPricing] = &[
    TokenPricing {
        model: "deepseek-flash",
        peak: TokenRates {
            cache_hit_usd_nanos: 6,
            cache_miss_usd_nanos: 300,
            output_usd_nanos: 1_200,
        },
        off_peak: TokenRates {
            cache_hit_usd_nanos: 3,
            cache_miss_usd_nanos: 150,
            output_usd_nanos: 600,
        },
    },
    TokenPricing {
        model: "deepseek-v4-flash",
        peak: TokenRates {
            cache_hit_usd_nanos: 6,
            cache_miss_usd_nanos: 300,
            output_usd_nanos: 1_200,
        },
        off_peak: TokenRates {
            cache_hit_usd_nanos: 3,
            cache_miss_usd_nanos: 150,
            output_usd_nanos: 600,
        },
    },
    TokenPricing {
        model: "deepseek-v4-flash-vision-exp",
        peak: TokenRates {
            cache_hit_usd_nanos: 6,
            cache_miss_usd_nanos: 300,
            output_usd_nanos: 1_200,
        },
        off_peak: TokenRates {
            cache_hit_usd_nanos: 3,
            cache_miss_usd_nanos: 150,
            output_usd_nanos: 600,
        },
    },
    TokenPricing {
        model: "deepseek-v4-pro",
        peak: TokenRates {
            cache_hit_usd_nanos: 44,
            cache_miss_usd_nanos: 1_320,
            output_usd_nanos: 3_960,
        },
        off_peak: TokenRates {
            cache_hit_usd_nanos: 22,
            cache_miss_usd_nanos: 660,
            output_usd_nanos: 1_980,
        },
    },
];

/// Chinese public holidays in 2026, from the State Council holiday arrangement
/// (off-day dates only; weekends are off-peak regardless).
///
/// DeepSeek bills all hours of a Chinese public holiday at off-peak rates.
/// Update this list when the State Council publishes the next year's arrangement;
/// a missing entry only misclassifies billing on that holiday's weekday hours.
const CHINA_PUBLIC_HOLIDAYS_2026: &[&str] = &[
    "2026-01-01",
    "2026-01-02",
    "2026-01-03",
    "2026-02-15",
    "2026-02-16",
    "2026-02-17",
    "2026-02-18",
    "2026-02-19",
    "2026-02-20",
    "2026-02-21",
    "2026-02-22",
    "2026-02-23",
    "2026-04-04",
    "2026-04-05",
    "2026-04-06",
    "2026-05-01",
    "2026-05-02",
    "2026-05-03",
    "2026-05-04",
    "2026-05-05",
    "2026-06-19",
    "2026-06-20",
    "2026-06-21",
    "2026-09-25",
    "2026-09-26",
    "2026-09-27",
    "2026-10-01",
    "2026-10-02",
    "2026-10-03",
    "2026-10-04",
    "2026-10-05",
    "2026-10-06",
    "2026-10-07",
];

/// Whether DeepSeek charges the peak tariff for a request started at `utc_seconds`.
///
/// Peak hours are 01:00-04:00 and 06:00-10:00 UTC, Monday through Friday, excluding
/// Chinese public holidays; every other hour is off-peak, including weekends and
/// public holidays in full. The exact hour boundary convention is unknown, so the
/// window is treated as half-open: 10:00 UTC already bills off-peak, and the cost
/// difference for a request straddling a boundary is a few tokens' worth.
///
/// ASSUMPTION: The tariff is chosen from the request start time recorded in the audit
/// event. If DeepSeek instead bills by completion time, a request that spans a
/// boundary window is charged one tariff away from DeepSeek's; the price multiplier
/// applied to our users keeps that difference within the platform margin.
pub fn is_peak(utc_seconds: i64) -> bool {
    let Some(datetime) = chrono::DateTime::from_timestamp(utc_seconds, 0) else {
        return false;
    };
    if matches!(datetime.weekday(), Weekday::Sat | Weekday::Sun) {
        return false;
    }
    let date = datetime.format("%Y-%m-%d").to_string();
    if CHINA_PUBLIC_HOLIDAYS_2026.contains(&date.as_str()) {
        return false;
    }
    let minutes = datetime.hour() * 60 + datetime.minute();
    (60..240).contains(&minutes) || (360..600).contains(&minutes)
}

/// Official DeepSeek cost of one completed request in USD nanodollars.
pub fn official_cost_usd_nanos(model: &str, usage: Usage, peak: bool) -> Option<i64> {
    let pricing = TOKEN_PRICING
        .iter()
        .find(|pricing| pricing.model == model)?;
    let rates = if peak { pricing.peak } else { pricing.off_peak };
    let cached_tokens = usage.cached_tokens.clamp(0, usage.input_tokens.max(0));
    let uncached_tokens = usage.input_tokens.saturating_sub(cached_tokens);
    Some(
        uncached_tokens
            .saturating_mul(rates.cache_miss_usd_nanos)
            .saturating_add(cached_tokens.saturating_mul(rates.cache_hit_usd_nanos))
            .saturating_add(
                usage
                    .output_tokens
                    .max(0)
                    .saturating_mul(rates.output_usd_nanos),
            ),
    )
}

/// Actual amount charged to the user: the official cost scaled by the platform
/// price multiplier (a ratio in USD nanodollars, where [`USD_NANOS`] means 1x).
pub fn apply_price_multiplier(official_cost_usd_nanos: i64, price_multiplier_nanos: i64) -> i64 {
    let actual_cost_usd_nanos = i128::from(official_cost_usd_nanos.max(0))
        * i128::from(price_multiplier_nanos.max(0))
        / i128::from(USD_NANOS);
    i64::try_from(actual_cost_usd_nanos).expect("configured price multiplier keeps costs in range")
}

#[derive(Serialize)]
pub struct ModelPriceRates {
    pub cache_hit_usd_nanos: i64,
    pub cache_miss_usd_nanos: i64,
    pub output_usd_nanos: i64,
}

#[derive(Serialize)]
pub struct ModelPrice {
    pub model: &'static str,
    pub peak: ModelPriceRates,
    pub off_peak: ModelPriceRates,
}

pub fn official_model_prices(available_model_ids: &[String]) -> Vec<ModelPrice> {
    TOKEN_PRICING
        .iter()
        .filter(|pricing| {
            available_model_ids
                .iter()
                .any(|model| model == pricing.model)
        })
        .map(|pricing| ModelPrice {
            model: pricing.model,
            peak: model_price_rates(pricing.peak),
            off_peak: model_price_rates(pricing.off_peak),
        })
        .collect()
}

fn model_price_rates(rates: TokenRates) -> ModelPriceRates {
    ModelPriceRates {
        cache_hit_usd_nanos: rates.cache_hit_usd_nanos * 1_000_000,
        cache_miss_usd_nanos: rates.cache_miss_usd_nanos * 1_000_000,
        output_usd_nanos: rates.output_usd_nanos * 1_000_000,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, TimeZone, Utc};

    fn at(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> i64 {
        Utc.from_utc_datetime(
            &NaiveDate::from_ymd_opt(year, month, day)
                .unwrap()
                .and_hms_opt(hour, minute, 0)
                .unwrap(),
        )
        .timestamp()
    }

    #[test]
    fn peak_windows_follow_the_utc_week() {
        // Monday 2026-09-28 02:00 UTC is inside the first peak window.
        assert!(is_peak(at(2026, 9, 28, 2, 0)));
        // 06:30 UTC is inside the second peak window.
        assert!(is_peak(at(2026, 9, 28, 6, 30)));
        // 04:00 and 05:00 are between the windows.
        assert!(!is_peak(at(2026, 9, 28, 4, 0)));
        assert!(!is_peak(at(2026, 9, 28, 5, 0)));
        // 01:00 opens the window; 10:00 closes the second window.
        assert!(is_peak(at(2026, 9, 28, 1, 0)));
        assert!(!is_peak(at(2026, 9, 28, 10, 0)));
        // Saturday and Sunday are off-peak in full.
        assert!(!is_peak(at(2026, 9, 26, 2, 0)));
        assert!(!is_peak(at(2026, 9, 27, 6, 0)));
    }

    #[test]
    fn chinese_public_holidays_are_off_peak_in_full() {
        // Mid-Autumn Festival 2026-09-25 is a Friday.
        assert!(!is_peak(at(2026, 9, 25, 2, 0)));
        // National Day 2026-10-01 is a Thursday.
        assert!(!is_peak(at(2026, 10, 1, 6, 0)));
    }

    #[test]
    fn flash_token_cost_matches_the_published_peak_price() {
        // 1M cache-miss input tokens at $0.30/M.
        let usage = Usage {
            input_tokens: 1_000_000,
            output_tokens: 0,
            cached_tokens: 0,
        };
        assert_eq!(
            official_cost_usd_nanos("deepseek-flash", usage, true),
            Some(300_000_000)
        );
        assert_eq!(
            official_cost_usd_nanos("deepseek-flash", usage, false),
            Some(150_000_000)
        );
        // 1M cache-hit input tokens at $0.006/M.
        let usage = Usage {
            input_tokens: 1_000_000,
            output_tokens: 0,
            cached_tokens: 1_000_000,
        };
        assert_eq!(
            official_cost_usd_nanos("deepseek-flash", usage, true),
            Some(6_000_000)
        );
        // 1M output tokens at $1.20/M.
        let usage = Usage {
            input_tokens: 0,
            output_tokens: 1_000_000,
            cached_tokens: 0,
        };
        assert_eq!(
            official_cost_usd_nanos("deepseek-flash", usage, true),
            Some(1_200_000_000)
        );
    }

    #[test]
    fn pro_token_cost_matches_the_published_rates() {
        let usage = Usage {
            input_tokens: 500,
            output_tokens: 1_000,
            cached_tokens: 200,
        };
        // 300 uncached at $1.32/M + 200 cached at $0.044/M + 1000 output at $3.96/M.
        assert_eq!(
            official_cost_usd_nanos("deepseek-v4-pro", usage, true),
            Some(300 * 1_320 + 200 * 44 + 1_000 * 3_960)
        );
        assert_eq!(
            official_cost_usd_nanos("deepseek-v4-pro", usage, false),
            Some(300 * 660 + 200 * 22 + 1_000 * 1_980)
        );
    }

    #[test]
    fn retired_flash_names_bill_at_the_flash_price() {
        let usage = Usage {
            input_tokens: 1_000_000,
            output_tokens: 0,
            cached_tokens: 0,
        };
        assert_eq!(
            official_cost_usd_nanos("deepseek-v4-flash", usage, true),
            official_cost_usd_nanos("deepseek-flash", usage, true)
        );
        assert_eq!(
            official_cost_usd_nanos("deepseek-v4-flash-vision-exp", usage, true),
            official_cost_usd_nanos("deepseek-flash", usage, true)
        );
    }

    #[test]
    fn unknown_models_have_no_official_price() {
        let usage = Usage::default();
        assert_eq!(official_cost_usd_nanos("gpt-5.4", usage, true), None);
    }

    #[test]
    fn price_multiplier_scales_the_official_cost() {
        // Default multiplier 0.1 charges a tenth of the official cost.
        assert_eq!(apply_price_multiplier(11, 100_000_000), 1);
        // A 2x multiplier charges double; USD_NANOS means exactly 1x.
        assert_eq!(apply_price_multiplier(11, 2 * USD_NANOS), 22);
        assert_eq!(apply_price_multiplier(11, USD_NANOS), 11);
    }

    #[test]
    fn price_snapshot_exposes_millions_of_tokens() {
        let prices = official_model_prices(&["deepseek-flash".to_owned()]);
        assert_eq!(prices.len(), 1);
        assert_eq!(prices[0].peak.cache_miss_usd_nanos, 300_000_000);
        assert_eq!(prices[0].off_peak.cache_miss_usd_nanos, 150_000_000);
        assert_eq!(prices[0].peak.output_usd_nanos, 1_200_000_000);
    }
}
