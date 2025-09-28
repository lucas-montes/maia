use rand::prelude::*;
use rand_distr::{Distribution, Normal, Uniform};
use rust_decimal::Decimal;
use rust_decimal::dec;
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use std::fmt;
use std::fmt::Display;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::cmp::Ordering;

fn write_totals_csv(
    filename: &str,
    results: impl Iterator<Item = (String, YearResult)>,
) -> std::io::Result<()> {
    let mut writer = BufWriter::new(File::create(filename)?);
    writeln!(writer, "scenario,year,total")?;
    for (scenario, year_result) in results {
        writeln!(
            writer,
            "{},{},{}",
            scenario, year_result.year, year_result.total
        )?;
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct ScenarioParams {
    portfolio: PortfolioParams,
    expenses: Expenses,
}

#[derive(Debug, Clone)]
struct ScenarioSummary {
    params: ScenarioParams,
    name: String,
    final_total: f64,
    years_to_million: Option<u32>,
}

#[derive(Debug)]
struct SimulationResult {
    simulation_name: String,
    best: ScenarioSummary,
    worst: ScenarioSummary,
    median: ScenarioSummary,
    q1: ScenarioSummary,
    q3: ScenarioSummary,
    average_final_total: f64,
    median_final_total: f64,
    stddev_final_total: f64,
    time_to_million_best: Option<u32>,
    time_to_million_worst: Option<u32>,
    time_to_million_median: Option<u32>,
    time_to_million_q1: Option<u32>,
    time_to_million_q3: Option<u32>,
    time_to_million_average: Option<f64>,
    count_reached_million: usize,
    total_scenarios: usize,
    duration_secs: f64,
}

impl fmt::Display for SimulationResult {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "\n{:-^172}", format!(" Simulation Summary [{}] ", self.simulation_name))?;
        writeln!(f, "{:<8} {:>13} {:>10} {:>8} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9}",
            "Type", "Final Total", "Years $1M", "Thresh", "FixedPct", "Inflation", "FixRet", "VarRet", "Rent", "Food", "Transp", "Util", "Entmt", "Others")?;
        writeln!(f, "{:-<172}", "")?;
        let em_dash = "—";
        let mut print_row = |label: &str, s: &ScenarioSummary| {
            let p = &s.params.portfolio;
            let e = &s.params.expenses;
            writeln!(f, "{:<8} {:>13.2} {:>10} {:>8.0} {:>9.3} {:>9.3} {:>9.3} {:>9.3} {:>9.2} {:>9.2} {:>9.2} {:>9.2} {:>9.2} {:>9.2}",
                label,
                s.final_total,
                s.years_to_million.map_or(em_dash.to_string(), |y| y.to_string()),
                p.savings_threshold,
                p.fixed_pct,
                p.inflation,
                p.fixed_return,
                p.variable_return,
                e.rent,
                e.food,
                e.transport,
                e.utilities,
                e.entertainment,
                e.others
            )
        };
        print_row("Best", &self.best)?;
        print_row("Worst", &self.worst)?;
        print_row("Median", &self.median)?;
        print_row("Q1", &self.q1)?;
        print_row("Q3", &self.q3)?;
        writeln!(f, "{:-<172}", "")?;
        writeln!(f, "{:<8} {:>13.2} {:>10} {:>8} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9}",
            "Avg", self.average_final_total, em_dash, em_dash, em_dash, em_dash, em_dash, em_dash, em_dash, em_dash, em_dash, em_dash, em_dash, em_dash)?;
        writeln!(f, "{:<8} {:>13.2} {:>10} {:>8} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9}",
            "Stddev", self.stddev_final_total, em_dash, em_dash, em_dash, em_dash, em_dash, em_dash, em_dash, em_dash, em_dash, em_dash, em_dash, em_dash)?;
        writeln!(f, "Scenarios ≥ $1M: {:<5}   Total: {:<5}   Duration (s): {:.2}", self.count_reached_million, self.total_scenarios, self.duration_secs)?;
        Ok(())
    }
}

#[derive(Debug)]
struct ScenarioResult {
    params: ScenarioParams,
    results: Vec<YearResult>,
}

impl ScenarioResult {
    fn new(params: ScenarioParams) -> Self {
        Self {
            params,
            results: Vec::new(),
        }
    }
    fn add_year_result(&mut self, year_result: YearResult) {
        self.results.push(year_result);
    }
}

struct StreamingStats {
    count: usize,
    mean: f64,
    m2: f64,
    min: f64,
    max: f64,
    sum: f64,
}

impl StreamingStats {
    fn new() -> Self {
        Self {
            count: 0,
            mean: 0.0,
            m2: 0.0,
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
            sum: 0.0,
        }
    }
    fn update(&mut self, value: f64) {
        self.count += 1;
        let delta = value - self.mean;
        self.mean += delta / self.count as f64;
        let delta2 = value - self.mean;
        self.m2 += delta * delta2;
        self.sum += value;
        if value < self.min { self.min = value; }
        if value > self.max { self.max = value; }
    }
    fn stddev(&self) -> f64 {
        if self.count < 2 { 0.0 } else { (self.m2 / (self.count as f64 - 1.0)).sqrt() }
    }
}

struct Histogram {
    bins: Vec<usize>,
    min: f64,
    max: f64,
}

impl Histogram {
    fn new(min: f64, max: f64, num_bins: usize) -> Self {
        Self {
            bins: vec![0; num_bins],
            min,
            max,
        }
    }
    fn add(&mut self, value: f64) {
        let num_bins = self.bins.len();
        let idx = if self.max == self.min {
            0
        } else {
            let pos = ((value - self.min) / (self.max - self.min) * (num_bins as f64)).floor() as usize;
            pos.min(num_bins - 1)
        };
        self.bins[idx] += 1;
    }
    fn quantile(&self, q: f64) -> usize {
        let total: usize = self.bins.iter().sum();
        let target = (q * total as f64).ceil() as usize;
        let mut acc = 0;
        for (i, &count) in self.bins.iter().enumerate() {
            acc += count;
            if acc >= target {
                return i;
            }
        }
        self.bins.len() - 1
    }
    fn value_at_bin(&self, bin: usize) -> f64 {
        let num_bins = self.bins.len();
        self.min + (self.max - self.min) * (bin as f64 + 0.5) / num_bins as f64
    }
}

fn analyze_histogram(
    stats: &StreamingStats,
    histogram: &Histogram,
    best: &ScenarioSummary,
    worst: &ScenarioSummary,
    count_reached_million: usize,
    total_scenarios: usize,
    duration_secs: f64,
) -> SimulationResult {
    let median_bin = histogram.quantile(0.5);
    let q1_bin = histogram.quantile(0.25);
    let q3_bin = histogram.quantile(0.75);
    let median_total = histogram.value_at_bin(median_bin);
    let q1_total = histogram.value_at_bin(q1_bin);
    let q3_total = histogram.value_at_bin(q3_bin);
    SimulationResult {
        simulation_name: String::from("Simulation"),
        best: best.clone(),
        worst: worst.clone(),
        median: ScenarioSummary {
            final_total: median_total,
            ..best.clone()
        },
        q1: ScenarioSummary {
            final_total: q1_total,
            ..best.clone()
        },
        q3: ScenarioSummary {
            final_total: q3_total,
            ..best.clone()
        },
        average_final_total: stats.mean,
        median_final_total: median_total,
        stddev_final_total: stats.stddev(),
        time_to_million_best: best.years_to_million,
        time_to_million_worst: worst.years_to_million,
        time_to_million_median: None,
        time_to_million_q1: None,
        time_to_million_q3: None,
        time_to_million_average: None,
        count_reached_million,
        total_scenarios,
        duration_secs,
    }
}

fn analyze_scenarios(scenarios: &[ScenarioResult], duration_secs: f64) -> SimulationResult {
    let mut summaries = Vec::new();
    let mut totals = Vec::new();
    let mut years_to_million = Vec::new();
    let mut years_to_million_vec = Vec::new();

    for scenario in scenarios {
        let final_total = scenario
            .results
            .last()
            .map(|y| y.total)
            .unwrap_or_default()
            .to_f64()
            .unwrap_or(0.0);
        let year_million = scenario
            .results
            .iter()
            .find(|y| y.total >= dec!(1_000_000.0))
            .map(|y| y.year);
        if let Some(y) = year_million {
            years_to_million.push(y);
            years_to_million_vec.push(y as f64);
        } else {
            years_to_million_vec.push(0.0);
        }
        summaries.push(ScenarioSummary {
            params: scenario.params.clone(),
            name: format!("{}", scenario.params.portfolio),
            final_total,
            years_to_million: year_million,
        });
        totals.push(final_total);
    }
    let mut sorted = summaries.clone();
    sorted.sort_by(|a, b| a.final_total.partial_cmp(&b.final_total).unwrap());
    let mid = sorted.len() / 2;
    let q1_idx = sorted.len() / 4;
    let q3_idx = 3 * sorted.len() / 4;
    let median = if sorted.is_empty() {
        summaries[0].clone()
    } else {
        sorted[mid].clone()
    };
    let q1 = if sorted.is_empty() {
        summaries[0].clone()
    } else {
        sorted[q1_idx].clone()
    };
    let q3 = if sorted.is_empty() {
        summaries[0].clone()
    } else {
        sorted[q3_idx].clone()
    };
    let best = sorted.last().cloned().unwrap_or_else(|| summaries[0].clone());
    let worst = sorted.first().cloned().unwrap_or_else(|| summaries[0].clone());
    let average_final_total = totals.iter().sum::<f64>() / totals.len().max(1) as f64;
    let median_final_total = if sorted.is_empty() {
        0.0
    } else if sorted.len() % 2 == 0 {
        (sorted[mid - 1].final_total + sorted[mid].final_total) / 2.0
    } else {
        sorted[mid].final_total
    };
    let stddev_final_total = {
        let mean = average_final_total;
        let var = totals.iter().map(|t| (t - mean).powi(2)).sum::<f64>() / totals.len().max(1) as f64;
        var.sqrt()
    };
    let mut years_sorted = years_to_million_vec.clone();
    years_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mid_y = years_sorted.len() / 2;
    let q1_y = years_sorted.len() / 4;
    let q3_y = 3 * years_sorted.len() / 4;
    let time_to_million_median = if years_sorted.is_empty() {
        None
    } else if years_sorted.len() % 2 == 0 {
        Some((years_sorted[mid_y - 1] + years_sorted[mid_y]) as u32 / 2)
    } else {
        Some(years_sorted[mid_y] as u32)
    };
    let time_to_million_q1 = if years_sorted.is_empty() {
        None
    } else {
        Some(years_sorted[q1_y] as u32)
    };
    let time_to_million_q3 = if years_sorted.is_empty() {
        None
    } else {
        Some(years_sorted[q3_y] as u32)
    };
    let time_to_million_best = years_sorted.iter().cloned().filter(|&y| y > 0.0).min_by(|a, b| a.partial_cmp(b).unwrap()).map(|y| y as u32);
    let time_to_million_worst = years_sorted.iter().cloned().filter(|&y| y > 0.0).max_by(|a, b| a.partial_cmp(b).unwrap()).map(|y| y as u32);
    let time_to_million_average = if !years_to_million.is_empty() {
        Some((years_to_million.iter().sum::<u32>() as f64) / years_to_million.len() as f64)
    } else {
        None
    };
    let count_reached_million = years_to_million.len();
    SimulationResult {
        simulation_name: String::from("Simulation"),
        best,
        worst,
        median,
        q1,
        q3,
        average_final_total,
        median_final_total,
        stddev_final_total,
        time_to_million_best,
        time_to_million_worst,
        time_to_million_median,
        time_to_million_q1,
        time_to_million_q3,
        time_to_million_average,
        count_reached_million,
        total_scenarios: scenarios.len(),
        duration_secs,
    }
}

#[derive(Debug)]
struct YearResult {
    year: u32,
    savings: Decimal,
    investments: Decimal,
    total: Decimal,
}

impl fmt::Display for YearResult {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Year {:>2}: Savings = {:>10.2}, Investments = {:>10.2}, Total = {:>10.2}",
            self.year,
            self.savings.round_dp(2),
            self.investments.round_dp(2),
            self.total.round_dp(2)
        )
    }
}

#[derive(Debug, Clone)]
struct Revenue(Decimal);

impl Revenue {
    fn new(monthly_income: Decimal) -> Self {
        Self(monthly_income)
    }

    fn annual_income(&self) -> Decimal {
        self.0 * dec!(12)
    }
}

struct RevenueIterator {
    incomes: Vec<Decimal>,
    idx: usize,
}

impl RevenueIterator {
    fn new(incomes: Vec<Decimal>) -> Self {
        Self { incomes, idx: 0 }
    }
}

impl Iterator for RevenueIterator {
    type Item = Revenue;
    fn next(&mut self) -> Option<Self::Item> {
        if self.idx >= self.incomes.len() {
            return None;
        }
        let revenue = Revenue::new(self.incomes[self.idx]);
        self.idx += 1;
        Some(revenue)
    }
}

#[derive(Debug, Clone)]
struct Expenses {
    rent: Decimal,
    food: Decimal,
    transport: Decimal,
    utilities: Decimal,
    entertainment: Decimal,
    others: Decimal,
}

impl Expenses {
    fn total(&self) -> Decimal {
        self.rent + self.food + self.transport + self.utilities + self.entertainment + self.others
    }

    fn annual(&self) -> Decimal {
        self.total() * dec!(12)
    }
}

#[derive(Debug, Clone)]
struct NormalDist {
    mean: f64,
    stddev: f64,
}

impl NormalDist {
    fn new(mean: f64, stddev: f64) -> Self {
        Self { mean, stddev }
    }
    fn sample(&self, rng: &mut impl Rng) -> Decimal {
        let dist = Normal::new(self.mean, self.stddev).unwrap();
        Decimal::from_f64(dist.sample(rng)).unwrap()
    }
}

#[derive(Debug, Clone)]
struct UniformDist {
    min: f64,
    max: f64,
}

impl UniformDist {
    fn new(min: f64, max: f64) -> Self {
        Self { min, max }
    }
    fn sample(&self, rng: &mut impl Rng) -> Decimal {
        let dist = Uniform::new(self.min, self.max);
        Decimal::from_f64(dist.sample(rng)).unwrap()
    }
}

#[derive(Debug, Clone)]
struct ExpensesDistribution {
    rent: NormalDist,
    food: NormalDist,
    transport: NormalDist,
    utilities: NormalDist,
    entertainment: NormalDist,
    others: Vec<Decimal>, // hardcoded values
}

impl ExpensesDistribution {
    fn new(
        rent: NormalDist,
        food: NormalDist,
        transport: NormalDist,
        utilities: NormalDist,
        entertainment: NormalDist,
        others: Vec<Decimal>,
    ) -> Self {
        Self {
            rent,
            food,
            transport,
            utilities,
            entertainment,
            others,
        }
    }
}

#[derive(Debug, Clone)]
struct ExpensesIterator {
    dist: ExpensesDistribution,
    rng: ThreadRng,
    n: usize,
    idx: usize,
}

impl ExpensesIterator {
    fn new(dist: ExpensesDistribution, n: usize) -> Self {
        Self {
            dist,
            rng: rand::thread_rng(),
            n,
            idx: 0,
        }
    }
}

impl Iterator for ExpensesIterator {
    type Item = Expenses;
    fn next(&mut self) -> Option<Self::Item> {
        if self.idx >= self.n {
            return None;
        }
        let others = self.dist.others[self.idx % self.dist.others.len()];
        let expenses = Expenses {
            rent: self.dist.rent.sample(&mut self.rng),
            food: self.dist.food.sample(&mut self.rng),
            transport: self.dist.transport.sample(&mut self.rng),
            utilities: self.dist.utilities.sample(&mut self.rng),
            entertainment: self.dist.entertainment.sample(&mut self.rng),
            others,
        };
        self.idx += 1;
        Some(expenses)
    }
}

fn calculate_net_income(gross_income: Decimal, expenses: &Expenses) -> Decimal {
    let cotisations = dec!(0.212);
    let cfp_taxes = dec!(0.001);
    let taxes_frais_de_chambre = dec!(0.0004);
    let cotisations_et_contributions = (gross_income * cotisations)
        + (gross_income * cfp_taxes)
        + (gross_income * taxes_frais_de_chambre).ceil();
    let revenue_net = gross_income - cotisations_et_contributions;
    let abbatement = dec!(0.5);
    let revenue_imposable = gross_income * abbatement;
    let tax_rate = if revenue_imposable <= dec!(3000) {
        dec!(0.1010)
    } else {
        dec!(0.1163)
    };
    let impot_sur_le_revenu = revenue_imposable * tax_rate;

    revenue_net - impot_sur_le_revenu - expenses.annual()
}

#[derive(Debug, Clone)]
struct PortfolioParams {
    years: u32,
    initial_savings: Decimal,
    initial_investments: Decimal,
    savings_threshold: Decimal,
    fixed_pct: Decimal,
    inflation: Decimal,
    fixed_return: Decimal,
    variable_return: Decimal,
}

impl Display for PortfolioParams {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "thresh:{}_fixedpct:{}_infl:{}_fixret:{}_varret:{}",
            self.savings_threshold,
            self.fixed_pct,
            self.inflation,
            self.fixed_return,
            self.variable_return
        )
    }
}

#[derive(Debug, Clone)]
struct PortfolioParamDistribution {
    savings_threshold: Vec<Decimal>, // hardcoded
    fixed_pct: UniformDist,
    inflation: UniformDist,
    fixed_return: UniformDist,
    variable_return: UniformDist,
    years: u32,
    initial_savings: Decimal,
    initial_investments: Decimal,
}

impl PortfolioParamDistribution {
    fn new(
        savings_threshold: Vec<Decimal>,
        fixed_pct: UniformDist,
        inflation: UniformDist,
        fixed_return: UniformDist,
        variable_return: UniformDist,
        years: u32,
        initial_savings: Decimal,
        initial_investments: Decimal,
    ) -> Self {
        Self {
            savings_threshold,
            fixed_pct,
            inflation,
            fixed_return,
            variable_return,
            years,
            initial_savings,
            initial_investments,
        }
    }
}

#[derive(Debug, Clone)]
struct PortfolioParamIterator {
    dist: PortfolioParamDistribution,
    rng: ThreadRng,
    n: usize,
    idx: usize,
}

impl PortfolioParamIterator {
    fn new(dist: PortfolioParamDistribution, n: usize) -> Self {
        Self {
            dist,
            rng: rand::thread_rng(),
            n,
            idx: 0,
        }
    }
}

impl Iterator for PortfolioParamIterator {
    type Item = PortfolioParams;
    fn next(&mut self) -> Option<Self::Item> {
        if self.idx >= self.n {
            return None;
        }
        let savings_threshold =
            self.dist.savings_threshold[self.idx % self.dist.savings_threshold.len()];
        let params = PortfolioParams {
            years: self.dist.years,
            initial_savings: self.dist.initial_savings,
            initial_investments: self.dist.initial_investments,
            savings_threshold,
            fixed_pct: self.dist.fixed_pct.sample(&mut self.rng),
            inflation: self.dist.inflation.sample(&mut self.rng),
            fixed_return: self.dist.fixed_return.sample(&mut self.rng),
            variable_return: self.dist.variable_return.sample(&mut self.rng),
        };
        self.idx += 1;
        Some(params)
    }
}

fn simulate_portfolio(
    params: &PortfolioParams,
    net_income: Decimal,
) -> impl Iterator<Item = YearResult> {
    let mut savings = params.initial_savings;
    let mut investments = params.initial_investments;

    let mut year = 1;
    std::iter::from_fn(move || {
        if year > params.years {
            return None;
        }
        let fixed_chg = (Decimal::ONE + params.fixed_return) / (Decimal::ONE + params.inflation);
        savings += net_income.max(Decimal::ZERO);
        savings *= fixed_chg;
        let overflow = if savings > params.savings_threshold {
            let extra = savings - params.savings_threshold;
            savings = params.savings_threshold;
            extra
        } else {
            Decimal::ZERO
        };
        let fixed_investment = (investments + overflow) * params.fixed_pct;
        let variable_investment = (investments + overflow) * (Decimal::ONE - params.fixed_pct);
        let new_fixed = fixed_investment * fixed_chg;
        let new_variable = variable_investment * (Decimal::ONE + params.variable_return)
            / (Decimal::ONE + params.inflation);
        investments = new_fixed + new_variable;
        let result = YearResult {
            year,
            savings: savings.round_dp(2),
            investments: investments.round_dp(2),
            total: (savings + investments).round_dp(2),
        };
        year += 1;
        Some(result)
    })
}

fn print_progress_bar(progress: usize, total: usize, width: usize) {
    let percent = (progress as f64 / total as f64) * 100.0;
    let filled = ((progress as f64 / total as f64) * width as f64).round() as usize;
    let bar = format!(
        "[{}>{}] {:>6.2}%",
        "=".repeat(filled),
        " ".repeat(width - filled),
        percent
    );
    print!("\r{}", bar);
    io::stdout().flush().unwrap();
}

fn main() {
    let n_sim = 5000;
    let expenses_dist = ExpensesDistribution::new(
        NormalDist::new(1000.0, 200.0), // rent
        NormalDist::new(400.0, 100.0),  // food
        NormalDist::new(60.0, 15.0),    // transport
        NormalDist::new(110.0, 10.0),   // utilities
        NormalDist::new(50.0, 20.0),    // entertainment
        vec![dec!(40)],                 // others
    );
    let portfolio_dist = PortfolioParamDistribution::new(
        vec![dec!(30000), dec!(20000)],
        UniformDist::new(0.7, 0.95),
        UniformDist::new(0.02, 0.07),
        UniformDist::new(0.01, 0.03),
        UniformDist::new(-0.12, 0.12),
        20,
        dec!(30357.27),
        dec!(29966.15),
    );
    let revenue_iter = RevenueIterator::new(vec![dec!(6700), dec!(7258.33)]);
    let expenses_iter = ExpensesIterator::new(expenses_dist, n_sim);
    let portfolio_param_iter = PortfolioParamIterator::new(portfolio_dist, n_sim);

    let total = revenue_iter.incomes.len() * n_sim * n_sim;
    let mut progress = 0;
    let start = std::time::Instant::now();

    let mut stats = StreamingStats::new();
    let mut best: Option<ScenarioSummary> = None;
    let mut worst: Option<ScenarioSummary> = None;
    let mut count_reached_million = 0;
    let mut min_total = f64::INFINITY;
    let mut max_total = f64::NEG_INFINITY;

    // First pass: find min/max for histogram
    for revenue in revenue_iter {
        for expenses in expenses_iter.clone() {
            let net_income = calculate_net_income(revenue.annual_income(), &expenses);
            for params in portfolio_param_iter.clone() {
                let mut scenario = ScenarioResult::new(ScenarioParams {
                    portfolio: params.clone(),
                    expenses: expenses.clone(),
                });
                for year_result in simulate_portfolio(&params, net_income) {
                    scenario.add_year_result(year_result);
                }
                let final_total = scenario.results.last().map(|y| y.total).unwrap_or_default().to_f64().unwrap_or(0.0);
                if final_total < min_total { min_total = final_total; }
                if final_total > max_total { max_total = final_total; }
                progress += 1;
                if progress % 1000 == 0 || progress == total {
                    print_progress_bar(progress, total, 40);
                }
            }
        }
    }
    let num_bins = 100;
    let mut histogram = Histogram::new(min_total, max_total, num_bins);
    let revenue_iter = RevenueIterator::new(vec![dec!(6700), dec!(7258.33)]);
    let mut progress = 0;
    println!("\nStarting second pass for stats and histogram...");
    // Second pass: stats and histogram
    for revenue in revenue_iter {
        for expenses in expenses_iter.clone() {
            let net_income = calculate_net_income(revenue.annual_income(), &expenses);
            for params in portfolio_param_iter.clone() {
                let mut scenario = ScenarioResult::new(ScenarioParams {
                    portfolio: params.clone(),
                    expenses: expenses.clone(),
                });
                for year_result in simulate_portfolio(&params, net_income) {
                    scenario.add_year_result(year_result);
                }
                let final_total = scenario.results.last().map(|y| y.total).unwrap_or_default().to_f64().unwrap_or(0.0);
                let year_million = scenario.results.iter().find(|y| y.total >= dec!(1_000_000.0)).map(|y| y.year);
                let summary = ScenarioSummary {
                    params: scenario.params.clone(),
                    name: format!("{}", scenario.params.portfolio),
                    final_total,
                    years_to_million: year_million,
                };
                stats.update(final_total);
                histogram.add(final_total);
                if year_million.is_some() {
                    count_reached_million += 1;
                }
                match &best {
                    None => best = Some(summary.clone()),
                    Some(b) if final_total > b.final_total => best = Some(summary.clone()),
                    _ => {}
                }
                match &worst {
                    None => worst = Some(summary.clone()),
                    Some(w) if final_total < w.final_total => worst = Some(summary.clone()),
                    _ => {}
                }
                progress += 1;
                if progress % 1000 == 0 || progress == total {
                    print_progress_bar(progress, total, 40);
                }
            }
        }
    }
    let duration_secs = start.elapsed().as_secs_f64();
    let sim_result = analyze_histogram(
        &stats,
        &histogram,
        &best.unwrap(),
        &worst.unwrap(),
        count_reached_million,
        total,
        duration_secs,
    );
    println!("{}", sim_result);
}
