// Appended to a freshly scaffolded Server Guest by new_project.rs, then run with cargo test.
#[cfg(test)]
mod observer_age_regression {
    use super::*;

    #[test]
    fn render_accepts_fractional_integer_and_null_observer_ages() {
        for age in [serde_json::json!(9.7), serde_json::json!(9), Value::Null] {
            let mut envelope = serde_json::to_value(RenderInput {
                v: 1,
                hook: "render".to_string(),
                observer: Some(Observer::default()),
                ..Default::default()
            })
            .unwrap();
            envelope["observer"]["ageSweeps"] = age.clone();
            let input: RenderInput = serde_json::from_str(&envelope.to_string()).unwrap();
            let decoded = serde_json::to_value(input.observer.as_ref().unwrap()).unwrap();
            assert_eq!(decoded["ageSweeps"].as_f64(), age.as_f64());
            assert!(render(input).unwrap().error.is_none());
        }
    }
}
