use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Unknown,
    Good,
    Moderate,
    Elevated,
    Unhealthy,
    VeryUnhealthy,
}

impl Status {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::Good => "Good",
            Self::Moderate => "Moderate",
            Self::Elevated => "Elevated",
            Self::Unhealthy => "Unhealthy",
            Self::VeryUnhealthy => "Very unhealthy",
        }
    }
}

pub fn classify_aqi(value: Option<f64>) -> Status {
    classify(
        value,
        &[
            (50.0, Status::Good),
            (100.0, Status::Moderate),
            (150.0, Status::Elevated),
            (200.0, Status::Unhealthy),
        ],
    )
}

pub fn classify_co2(value: Option<f64>) -> Status {
    classify(
        value,
        &[
            (800.0, Status::Good),
            (1000.0, Status::Moderate),
            (1500.0, Status::Elevated),
            (2000.0, Status::Unhealthy),
        ],
    )
}

pub fn classify_pm25(value: Option<f64>) -> Status {
    classify(
        value,
        &[
            (9.0, Status::Good),
            (35.4, Status::Moderate),
            (55.4, Status::Elevated),
            (125.4, Status::Unhealthy),
        ],
    )
}

pub fn classify_pm10(value: Option<f64>) -> Status {
    classify(
        value,
        &[
            (54.0, Status::Good),
            (154.0, Status::Moderate),
            (254.0, Status::Elevated),
            (354.0, Status::Unhealthy),
        ],
    )
}

pub fn classify_particles(value: Option<f64>) -> Status {
    classify(
        value,
        &[
            (500.0, Status::Good),
            (1000.0, Status::Moderate),
            (2000.0, Status::Elevated),
            (5000.0, Status::Unhealthy),
        ],
    )
}

pub fn classify_tvoc(value: Option<f64>) -> Status {
    classify(
        value,
        &[
            (100.0, Status::Good),
            (200.0, Status::Moderate),
            (300.0, Status::Elevated),
            (400.0, Status::Unhealthy),
        ],
    )
}

pub fn classify_nox(value: Option<f64>) -> Status {
    classify(
        value,
        &[
            (1.0, Status::Good),
            (20.0, Status::Moderate),
            (50.0, Status::Elevated),
            (100.0, Status::Unhealthy),
        ],
    )
}

pub fn classify_temperature(value: Option<f64>) -> Status {
    let Some(value) = value.filter(|value| value.is_finite()) else {
        return Status::Unknown;
    };

    if !(16.0..=30.0).contains(&value) {
        Status::Elevated
    } else if !(18.0..=26.0).contains(&value) {
        Status::Moderate
    } else {
        Status::Good
    }
}

pub fn classify_humidity(value: Option<f64>) -> Status {
    let Some(value) = value.filter(|value| value.is_finite()) else {
        return Status::Unknown;
    };

    if !(20.0..=80.0).contains(&value) {
        Status::Unhealthy
    } else if !(30.0..=60.0).contains(&value) {
        Status::Moderate
    } else {
        Status::Good
    }
}

fn classify(value: Option<f64>, breakpoints: &[(f64, Status)]) -> Status {
    let Some(value) = value.filter(|value| value.is_finite()) else {
        return Status::Unknown;
    };

    for (max, status) in breakpoints {
        if value <= *max {
            return *status;
        }
    }

    Status::VeryUnhealthy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_aqi_boundaries() {
        assert_eq!(classify_aqi(None), Status::Unknown);
        assert_eq!(classify_aqi(Some(50.0)), Status::Good);
        assert_eq!(classify_aqi(Some(51.0)), Status::Moderate);
        assert_eq!(classify_aqi(Some(101.0)), Status::Elevated);
        assert_eq!(classify_aqi(Some(151.0)), Status::Unhealthy);
        assert_eq!(classify_aqi(Some(201.0)), Status::VeryUnhealthy);
    }

    #[test]
    fn classifies_humidity_and_temperature_boundaries() {
        assert_eq!(classify_temperature(Some(22.0)), Status::Good);
        assert_eq!(classify_temperature(Some(17.0)), Status::Moderate);
        assert_eq!(classify_temperature(Some(31.0)), Status::Elevated);
        assert_eq!(classify_humidity(Some(45.0)), Status::Good);
        assert_eq!(classify_humidity(Some(25.0)), Status::Moderate);
        assert_eq!(classify_humidity(Some(85.0)), Status::Unhealthy);
    }

    #[test]
    fn classifies_all_metric_families() {
        assert_eq!(classify_co2(Some(801.0)), Status::Moderate);
        assert_eq!(classify_pm25(Some(55.5)), Status::Unhealthy);
        assert_eq!(classify_pm10(Some(355.0)), Status::VeryUnhealthy);
        assert_eq!(classify_particles(Some(1001.0)), Status::Elevated);
        assert_eq!(classify_tvoc(Some(401.0)), Status::VeryUnhealthy);
        assert_eq!(classify_nox(Some(21.0)), Status::Elevated);
    }
}
