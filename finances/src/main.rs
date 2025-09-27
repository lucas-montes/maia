use rust_decimal::Decimal;
use rust_decimal::dec;

use std::fmt;
use std::iter::StepBy;
use std::path::Iter;

#[derive(Debug)]
struct YearResult {
    year: u32,
    savings: f64,
    investments: f64,
    total: f64,
}

impl fmt::Display for YearResult {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Year {:>2}: Savings = {:>10.2}, Investments = {:>10.2}, Total = {:>10.2}",
            self.year, self.savings, self.investments, self.total
        )
    }
}
#[derive(Debug, Clone)]
struct PortfolioParams {
    years: u32,
    initial_savings: Decimal,
    initial_investments: Decimal,
    annual_contribution: Decimal,
    savings_threshold: Decimal,
    fixed_pct: Decimal,      // e.g. 0.6 for 60% fixed, 0.4 for 40% variable
    inflation: Decimal,
    fixed_return: Decimal,
    variable_return: Decimal,
}
struct PortfolioParamRanges {
    years: Vec<u32>,
    initial_savings: Vec<Decimal>,
    initial_investments: Vec<Decimal>,
    annual_contribution: Vec<Decimal>,
    savings_threshold: Vec<Decimal>,
    fixed_pct: Vec<Decimal>,
    inflation: Vec<Decimal>,
    fixed_return: Vec<Decimal>,
    variable_return: Vec<Decimal>,
    idx: [usize; 9],
    done: bool,
}

impl PortfolioParamRanges {
    fn new(
        years: Vec<u32>,
        initial_savings: Vec<Decimal>,
        initial_investments: Vec<Decimal>,
        annual_contribution: Vec<Decimal>,
        savings_threshold: Vec<Decimal>,
        fixed_pct: Vec<Decimal>,
        inflation: Vec<Decimal>,
        fixed_return: Vec<Decimal>,
        variable_return: Vec<Decimal>,
    ) -> Self {
        Self {
            years,
            initial_savings,
            initial_investments,
            annual_contribution,
            savings_threshold,
            fixed_pct,
            inflation,
            fixed_return,
            variable_return,
            idx: [0; 9],
            done: false,
        }
    }
}

impl Iterator for PortfolioParamRanges {
    type Item = PortfolioParams;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let params = PortfolioParams {
            years: self.years[self.idx[0]],
            initial_savings: self.initial_savings[self.idx[1]],
            initial_investments: self.initial_investments[self.idx[2]],
            annual_contribution: self.annual_contribution[self.idx[3]],
            savings_threshold: self.savings_threshold[self.idx[4]],
            fixed_pct: self.fixed_pct[self.idx[5]],
            inflation: self.inflation[self.idx[6]],
            fixed_return: self.fixed_return[self.idx[7]],
            variable_return: self.variable_return[self.idx[8]],
        };
        // Advance indices
        for i in (0..9).rev() {
            self.idx[i] += 1;
            let lens = [
                self.years.len(),
                self.initial_savings.len(),
                self.initial_investments.len(),
                self.annual_contribution.len(),
                self.savings_threshold.len(),
                self.fixed_pct.len(),
                self.inflation.len(),
                self.fixed_return.len(),
                self.variable_return.len(),
            ];
            if self.idx[i] < lens[i] {
                break;
            } else {
                self.idx[i] = 0;
                if i == 0 {
                    self.done = true;
                }
            }
        }
        Some(params)
    }
}

fn simulate_portfolio(params: &PortfolioParams) -> Vec<YearResult> {
    let mut results = Vec::new();

    let mut savings = params.initial_savings;
    let mut investments = params.initial_investments;

    for year in 1..=params.years {
        // Add contribution
        savings += params.annual_contribution;

        // Grow savings with real return (adjusted for inflation)
        savings *= (Decimal::ONE + params.fixed_return) / (Decimal::ONE + params.inflation);

        // Move excess above threshold into investments
        let overflow = if savings > params.savings_threshold {
            let extra = savings - params.savings_threshold;
            savings = params.savings_threshold;
            extra
        } else {
            Decimal::ZERO
        };

        // Split investments between fixed and variable
        let fixed_investment = (investments + overflow) * params.fixed_pct;
        let variable_investment = (investments + overflow) * (Decimal::ONE - params.fixed_pct);

        // Grow investments
        let new_fixed = fixed_investment * (Decimal::ONE + params.fixed_return) / (Decimal::ONE + params.inflation);
        let new_variable = variable_investment * (Decimal::ONE + params.variable_return) / (Decimal::ONE + params.inflation);

        investments = new_fixed + new_variable;

        // Save this year's result
        results.push(YearResult {
            year,
            savings: (savings.().unwrap() * 100.0).round() / 100.0,
            investments: (investments.to_f64().unwrap() * 100.0).round() / 100.0,
            total: ((savings + investments).to_f64().unwrap() * 100.0).round() / 100.0,
        });
    }

    results
}

fn calculate_net_income(income: Decimal) -> Decimal {
    let cotisations = dec!(0.212);

    let cfp_taxes = dec!(0.001);

    let taxes_frais_de_chambre = dec!(0.0004);

    let cotisations_et_contributions =
        (income * cotisations) + (income * cfp_taxes) + (income * taxes_frais_de_chambre).ceil();

    let revenue_net = income - cotisations_et_contributions;

    let abbatement = dec!(0.5);

    //TODO: check if is needed to round up to two decimals, it seems to be the case for all results actually
    let revenue_imposable = income * abbatement;

    //TODO: check how this percentage is calculated
    let tax_rate = if revenue_imposable <= dec!(3000) {
        dec!(0.1010)
    } else {
        dec!(0.1163)
    };

    let impot_sur_le_revenu = revenue_imposable * tax_rate;

    revenue_net - impot_sur_le_revenu
}

fn variations() -> Decimal {
    let income = dec!(6700);
    let rate = dec!(0.854515);

    let yearly_income = income * dec!(12);

    let net_income = calculate_net_income(yearly_income);
    dec!(12)
}

#[derive(Debug)]
struct Expenses {
    rent: Decimal,
    food: Decimal,
    transport: Decimal,
    utilities: Decimal,
    entertainment: Decimal,
    others: Decimal,
}

struct ExpensesCombinations {
    rent: Vec<u8>,
    food: Vec<u8>,
    transport: Vec<u8>,
    utilities: Vec<u8>,
    entertainment: Vec<u8>,
    others: Vec<u8>,
    idx: [usize; 6],
    done: bool,
}

impl ExpensesCombinations {
    fn new(
        rent: Vec<u8>,
        food: Vec<u8>,
        transport: Vec<u8>,
        utilities: Vec<u8>,
        entertainment: Vec<u8>,
        others: Vec<u8>,
    ) -> Self {
        Self {
            rent,
            food,
            transport,
            utilities,
            entertainment,
            others,
            idx: [0; 6],
            done: false,
        }
    }
}

impl Iterator for ExpensesCombinations {
    type Item = Expenses;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let combo = Expenses {
            rent: Decimal::from(self.rent[self.idx[0]]),
            food: Decimal::from(self.food[self.idx[1]]),
            transport: Decimal::from(self.transport[self.idx[2]]),
            utilities: Decimal::from(self.utilities[self.idx[3]]),
            entertainment: Decimal::from(self.entertainment[self.idx[4]]),
            others: Decimal::from(self.others[self.idx[5]]),
        };
        // Advance indices
        for i in (0..6).rev() {
            self.idx[i] += 1;
            if self.idx[i] < match i {
                0 => self.rent.len(),
                1 => self.food.len(),
                2 => self.transport.len(),
                3 => self.utilities.len(),
                4 => self.entertainment.len(),
                5 => self.others.len(),
                _ => unreachable!(),
            } {
                break;
            } else {
                self.idx[i] = 0;
                if i == 0 {
                    self.done = true;
                }
            }
        }
        Some(combo)
    }
}

fn main() {
     let param_ranges = PortfolioParamRanges::new(
        vec![10, 20], // years
        vec![dec!(10000)], // initial_savings
        vec![dec!(5000)], // initial_investments
        vec![dec!(1000)], // annual_contribution
        vec![dec!(5000)], // savings_threshold
        vec![dec!(0.6), dec!(0.8)], // fixed_pct
        vec![dec!(0.02), dec!(0.03)], // inflation
        vec![dec!(0.03), dec!(0.04)], // fixed_return
        vec![dec!(0.06), dec!(0.08)], // variable_return
    );
    for params in param_ranges.take(3) {
        let results = simulate_portfolio(&params);
        println!("Params: {:?}\nResults: {:?}", params, results);
    }
    let expenses = ExpensesCombinations::new(
        vec![0, 2, 4, 6, 8], // rent
        vec![0, 2, 4, 6, 8], // food
        vec![0, 2, 4, 6, 8], // transport
        vec![0, 2, 4, 6, 8], // utilities
        vec![0, 2, 4, 6, 8], // entertainment
        vec![0, 2, 4, 6, 8], // others
    );
    for combo in expenses.take(5) {
        println!("{:?}", combo);
    }
    println!("Result {:?}", variations());
}
