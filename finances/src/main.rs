use rand::prelude::*;
use rand_distr::{Distribution, Normal, Uniform};
use rust_decimal::Decimal;
use rust_decimal::dec;
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use std::fmt;
use std::fmt::Display;
use std::fs::File;
use std::io::{self, BufWriter, Write};

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

#[derive(Debug)]
struct SimulationResult {
    best: ScenarioSummary,
    worst: ScenarioSummary,
    average_final_total: f64,
    median_final_total: f64,
    stddev_final_total: f64,
    time_to_million_best: Option<u32>,
    time_to_million_worst: Option<u32>,
    time_to_million_average: Option<f64>,
    count_reached_million: usize,
    total_scenarios: usize,
}

impl fmt::Display for SimulationResult {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "\n{:-^80}", " Simulation Summary ")?;
        writeln!(
            f,
            "{:<20} {:>15} {:>15} {:>15}",
            "Metric", "Value", "Best", "Worst"
        )?;
        writeln!(f, "{:-<80}", "")?;

        writeln!(
            f,
            "{:<20} {:>15.2} {:>15.2} {:>15.2}",
            "Final Total", self.average_final_total, self.best.final_total, self.worst.final_total
        )?;
        writeln!(
            f,
            "{:<20} {:>15.2} {:>15.2} {:>15.2}",
            "Median Total", self.median_final_total, self.best.final_total, self.worst.final_total
        )?;
        writeln!(
            f,
            "{:<20} {:>15.2}",
            "Stddev Total", self.stddev_final_total
        )?;
        writeln!(
            f,
            "{:<20} {:>15}",
            "Scenarios ≥ $1M", self.count_reached_million
        )?;
        writeln!(f, "{:<20} {:>15}", "Total Scenarios", self.total_scenarios)?;
        writeln!(
            f,
            "{:<20} {:>15.2} {:>15} {:>15}",
            "Years to $1M",
            self.time_to_million_average.unwrap_or(0.0),
            self.time_to_million_best.unwrap_or(0),
            self.time_to_million_worst.unwrap_or(0)
        )?;
        writeln!(f, "{:-<80}", "")?;
        writeln!(
            f,
            "Best scenario: {:<20} \nWorst scenario: {:<20}",
            self.best.name, self.worst.name
        )
    }
}

#[derive(Debug, Clone)]
struct ScenarioSummary {
    name: String,
    final_total: f64,
    years_to_million: Option<u32>,
}

fn analyze_scenarios(scenarios: &[ScenarioResult]) -> SimulationResult {
    let mut summaries = Vec::new();
    let mut totals = Vec::new();
    let mut years_to_million = Vec::new();

    for scenario in scenarios {
        let final_total = scenario
            .results
            .last()
            .map(|y| y.total)
            .unwrap_or_default()
            .to_f64()
            .unwrap_or(0.0);
        let name = scenario.name.clone();
        let year_million = scenario
            .results
            .iter()
            .find(|y| y.total >= dec!(1_000_000.0))
            .map(|y| y.year);

        if let Some(y) = year_million {
            years_to_million.push(y);
        }

        summaries.push(ScenarioSummary {
            name,
            final_total,
            years_to_million: year_million,
        });
        totals.push(final_total);
    }

    // Sort summaries for best/worst
    let best = summaries
        .iter()
        .max_by(|a, b| a.final_total.partial_cmp(&b.final_total).unwrap())
        .unwrap()
        .to_owned();
    let worst = summaries
        .iter()
        .min_by(|a, b| a.final_total.partial_cmp(&b.final_total).unwrap())
        .unwrap()
        .to_owned();

    // Average, median, stddev
    let average_final_total = totals.iter().sum::<f64>() / totals.len().max(1) as f64;
    let median_final_total = {
        let mut sorted = totals.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mid = sorted.len() / 2;
        if sorted.is_empty() {
            0.0
        } else if sorted.len() % 2 == 0 {
            (sorted[mid - 1] + sorted[mid]) / 2.0
        } else {
            sorted[mid]
        }
    };
    let stddev_final_total = {
        let mean = average_final_total;
        let var =
            totals.iter().map(|t| (t - mean).powi(2)).sum::<f64>() / totals.len().max(1) as f64;
        var.sqrt()
    };

    let count_reached_million = years_to_million.len();
    let time_to_million_best = years_to_million.iter().min().copied();
    let time_to_million_worst = years_to_million.iter().max().copied();
    let time_to_million_average = if !years_to_million.is_empty() {
        Some((years_to_million.iter().sum::<u32>() as f64) / years_to_million.len() as f64)
    } else {
        None
    };

    SimulationResult {
        best,
        worst,
        average_final_total,
        median_final_total,
        stddev_final_total,
        time_to_million_best,
        time_to_million_worst,
        time_to_million_average,
        count_reached_million,
        total_scenarios: scenarios.len(),
    }
}

#[derive(Debug)]
struct ScenarioResult {
    name: String,
    results: Vec<YearResult>,
}

impl ScenarioResult {
    fn new(name: String) -> Self {
        Self {
            name,
            results: Vec::new(),
        }
    }
    fn add_year_result(&mut self, year_result: YearResult) {
        self.results.push(year_result);
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
    let n_sim = 3000;
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
    let mut all_results = Vec::new();

    let total = revenue_iter.incomes.len() * n_sim * n_sim;
    let mut progress = 0;

    for revenue in revenue_iter {
        for expenses in expenses_iter.clone() {
            let net_income = calculate_net_income(revenue.annual_income(), &expenses);
            for params in portfolio_param_iter.clone() {
                let scenario_name = params.to_string();
                let mut scenario = ScenarioResult::new(scenario_name);
                for year_result in simulate_portfolio(&params, net_income) {
                    scenario.add_year_result(year_result);
                }
                all_results.push(scenario);
                progress += 1;
                if progress % 1000 == 0 || progress == total {
                    print_progress_bar(progress, total, 40);
                }
            }
        }
    }
    let sim_result = analyze_scenarios(&all_results);
    println!("{}", sim_result);
}
