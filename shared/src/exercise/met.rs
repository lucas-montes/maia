pub struct Met {
    id: i64,
    activity: String,
    value: f32,
}

impl Met {
    pub fn new(id: i64, activity: String, value: f32) -> Self {
        Self { id, activity, value }
    }

    pub fn kcal_per_hour(&self, weight:f32) -> f32 {
        self.value * weight
    }
}
