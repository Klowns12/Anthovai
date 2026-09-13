//! A project, what it was budgeted, what it has cost, and what that means.

use anthovai_core::{CostEntryId, DocumentId, OrgId, PhaseId, ProjectId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::money::Money;

/// Over budget past this share of the budget, and it is worth saying so.
///
/// Not zero: every project crosses its own estimate by a little, and a system
/// that shouts at 100.1% is one nobody reads by the second week.
const OVER_BUDGET_WARN: f64 = 100.0;
const OVER_BUDGET_CRITICAL: f64 = 110.0;

/// How far money may run ahead of work before it is worth saying so, in
/// percentage points.
///
/// Some gap is normal — materials are paid for before they are installed. Ten
/// points is the width at which the contractors who described this problem
/// started calling it a problem.
const DRAW_AHEAD_WARN: f64 = 10.0;
const DRAW_AHEAD_CRITICAL: f64 = 25.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectStatus {
    Active,
    Closed,
}

/// A line of the estimate: what this category of work was supposed to cost.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BudgetLine {
    /// "งานโครงสร้าง", "ค่าแรง", "วัสดุก่อสร้าง" — the customer's own words.
    pub category: String,
    pub budgeted: Money,
}

/// One draw: a stage of the job, what it is worth, and how much of it is done.
///
/// `completion` is a judgement someone makes on site, not something a document
/// can report. It is the only number here a person has to supply, and the whole
/// early warning depends on it being honest.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Phase {
    pub id: PhaseId,
    pub name: String,
    /// What this phase is worth when finished.
    pub value: Money,
    /// How much has actually been invoiced or drawn against it.
    pub drawn: Money,
    /// 0.0–100.0, assessed on site.
    pub completion: f64,
}

impl Phase {
    /// What this phase has earned at its assessed completion.
    pub fn earned(&self) -> Money {
        let share = self.completion.clamp(0.0, 100.0) / 100.0;
        Money::from_satang((self.value.satang() as f64 * share).round() as i64)
    }
}

/// A cost that landed on the project, traceable back to the document it came
/// from.
///
/// `document_id` is not optional by accident: a figure in a cost report that
/// nobody can trace to a piece of paper is the thing this product replaces.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CostEntry {
    pub id: CostEntryId,
    pub document_id: DocumentId,
    pub category: String,
    pub amount: Money,
    /// False when the reading was not trusted — the arithmetic did not check
    /// out, or a person has not confirmed it yet. Such entries are counted
    /// separately rather than dropped: money that may have been spent is not
    /// the same as money that was not.
    pub confirmed: bool,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    pub org_id: OrgId,
    pub name: String,
    pub status: ProjectStatus,
    pub budget: Vec<BudgetLine>,
    pub phases: Vec<Phase>,
    pub costs: Vec<CostEntry>,
}

/// Something worth telling someone about, in words they can act on.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Alert {
    pub severity: Severity,
    /// Thai, because the person reading it is a project manager in Bangkok.
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Warning,
    Critical,
}

impl Project {
    pub fn budgeted(&self) -> Money {
        self.budget.iter().map(|b| b.budgeted).sum()
    }

    /// Cost that has been confirmed. What the project has definitely spent.
    pub fn spent(&self) -> Money {
        self.costs
            .iter()
            .filter(|c| c.confirmed)
            .map(|c| c.amount)
            .sum()
    }

    /// Cost read off documents that nobody has confirmed yet.
    ///
    /// Reported beside `spent` rather than folded into it: a manager deciding
    /// whether to keep buying needs to know both what is certain and what is
    /// probably also true.
    pub fn pending(&self) -> Money {
        self.costs
            .iter()
            .filter(|c| !c.confirmed)
            .map(|c| c.amount)
            .sum()
    }

    /// Confirmed plus pending — the number to steer by.
    pub fn committed(&self) -> Money {
        self.spent() + self.pending()
    }

    pub fn remaining(&self) -> Money {
        self.budgeted() - self.committed()
    }

    /// Spending against the budget, in percent. `None` with no budget set.
    pub fn spend_percent(&self) -> Option<f64> {
        self.committed().percent_of(self.budgeted())
    }

    /// What has been drawn across every phase, and what has been earned.
    pub fn drawn(&self) -> Money {
        self.phases.iter().map(|p| p.drawn).sum()
    }

    pub fn earned(&self) -> Money {
        self.phases.iter().map(|p| p.earned()).sum()
    }

    /// How far money has run ahead of work, in percentage points of the total
    /// phase value. Positive means paid ahead of built.
    ///
    /// `None` when no phase is worth anything, because then there is no work to
    /// be ahead of.
    pub fn draw_ahead_points(&self) -> Option<f64> {
        let total: Money = self.phases.iter().map(|p| p.value).sum();
        let drawn = self.drawn().percent_of(total)?;
        let earned = self.earned().percent_of(total)?;
        Some(((drawn - earned) * 10.0).round() / 10.0)
    }

    /// Which categories have spent more than they were given.
    ///
    /// A project can be inside its total and still have a category that has
    /// run away; by the time the total notices, the money is gone.
    pub fn categories_over_budget(&self) -> Vec<(String, Money, Money)> {
        self.budget
            .iter()
            .filter_map(|line| {
                let spent: Money = self
                    .costs
                    .iter()
                    .filter(|c| c.category == line.category)
                    .map(|c| c.amount)
                    .sum();
                (spent > line.budgeted).then(|| (line.category.clone(), line.budgeted, spent))
            })
            .collect()
    }

    /// Everything worth telling someone, worst first.
    pub fn alerts(&self) -> Vec<Alert> {
        let mut alerts = Vec::new();

        if let Some(percent) = self.spend_percent() {
            if percent >= OVER_BUDGET_CRITICAL {
                alerts.push(Alert {
                    severity: Severity::Critical,
                    message: format!(
                        "ต้นทุนถึง {:.1}% ของงบแล้ว (ใช้ไป {} จากงบ {}) — เกินงบ {}",
                        percent,
                        self.committed(),
                        self.budgeted(),
                        self.committed() - self.budgeted()
                    ),
                });
            } else if percent >= OVER_BUDGET_WARN {
                alerts.push(Alert {
                    severity: Severity::Warning,
                    message: format!(
                        "ต้นทุนถึง {:.1}% ของงบแล้ว (ใช้ไป {} จากงบ {})",
                        percent,
                        self.committed(),
                        self.budgeted()
                    ),
                });
            }
        }

        if let Some(points) = self.draw_ahead_points() {
            if points >= DRAW_AHEAD_CRITICAL {
                alerts.push(Alert {
                    severity: Severity::Critical,
                    message: format!(
                        "เบิกเงินไปแล้วมากกว่างานที่เสร็จ {:.1} จุด — ตรวจงวดงานก่อนจ่ายงวดถัดไป",
                        points
                    ),
                });
            } else if points >= DRAW_AHEAD_WARN {
                alerts.push(Alert {
                    severity: Severity::Warning,
                    message: format!("เบิกเงินนำงานที่เสร็จอยู่ {:.1} จุด", points),
                });
            }
        }

        for (category, budgeted, spent) in self.categories_over_budget() {
            alerts.push(Alert {
                severity: Severity::Warning,
                message: format!("หมวด \"{category}\" เกินงบแล้ว: ใช้ไป {spent} จากงบ {budgeted}"),
            });
        }

        if !self.pending().is_zero() {
            alerts.push(Alert {
                severity: Severity::Warning,
                message: format!(
                    "มีเอกสารมูลค่า {} ที่ยังไม่ได้ตรวจยืนยัน — ตัวเลขต้นทุนอาจยังไม่ครบ",
                    self.pending()
                ),
            });
        }

        alerts.sort_by_key(|a| std::cmp::Reverse(a.severity));
        alerts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(budget: &[(&str, i64)], phases: Vec<Phase>, costs: Vec<CostEntry>) -> Project {
        Project {
            id: ProjectId::new(),
            org_id: OrgId::new(),
            name: "บ้านพักอาศัย 2 ชั้น ซ.รามอินทรา 19".into(),
            status: ProjectStatus::Active,
            budget: budget
                .iter()
                .map(|(category, baht)| BudgetLine {
                    category: (*category).into(),
                    budgeted: Money::from_baht(*baht),
                })
                .collect(),
            phases,
            costs,
        }
    }

    fn phase(name: &str, value: i64, drawn: i64, completion: f64) -> Phase {
        Phase {
            id: PhaseId::new(),
            name: name.into(),
            value: Money::from_baht(value),
            drawn: Money::from_baht(drawn),
            completion,
        }
    }

    fn cost(category: &str, baht: i64, confirmed: bool) -> CostEntry {
        CostEntry {
            id: CostEntryId::new(),
            document_id: DocumentId::new(),
            category: category.into(),
            amount: Money::from_baht(baht),
            confirmed,
            recorded_at: Utc::now(),
        }
    }

    #[test]
    fn a_project_inside_its_budget_says_nothing() {
        let p = project(
            &[("วัสดุ", 500_000), ("ค่าแรง", 300_000)],
            vec![phase("งวดที่ 1", 400_000, 200_000, 50.0)],
            vec![cost("วัสดุ", 200_000, true)],
        );

        assert_eq!(p.budgeted(), Money::from_baht(800_000));
        assert_eq!(p.spent(), Money::from_baht(200_000));
        assert_eq!(p.remaining(), Money::from_baht(600_000));
        assert!(p.alerts().is_empty(), "{:?}", p.alerts());
    }

    #[test]
    fn money_that_is_not_confirmed_is_counted_but_kept_separate() {
        // A document read but not yet checked is money that may have been
        // spent. Folding it into `spent` would overstate certainty; dropping it
        // would understate the cost. It is reported as its own number.
        let p = project(
            &[("วัสดุ", 500_000)],
            vec![],
            vec![cost("วัสดุ", 100_000, true), cost("วัสดุ", 40_000, false)],
        );

        assert_eq!(p.spent(), Money::from_baht(100_000));
        assert_eq!(p.pending(), Money::from_baht(40_000));
        assert_eq!(p.committed(), Money::from_baht(140_000));

        let alerts = p.alerts();
        assert_eq!(alerts.len(), 1);
        assert!(
            alerts[0].message.contains("ยังไม่ได้ตรวจยืนยัน"),
            "{:?}",
            alerts[0]
        );
    }

    #[test]
    fn over_budget_is_reported_with_the_amount_not_only_the_percentage() {
        // "112%" makes a manager do arithmetic. "เกินงบ 120,000" does not.
        let p = project(
            &[("วัสดุ", 1_000_000)],
            vec![],
            vec![cost("วัสดุ", 1_120_000, true)],
        );

        let alerts = p.alerts();
        assert_eq!(alerts[0].severity, Severity::Critical);
        assert!(
            alerts[0].message.contains("112.0%"),
            "{}",
            alerts[0].message
        );
        assert!(
            alerts[0].message.contains("120,000.00"),
            "{}",
            alerts[0].message
        );
    }

    #[test]
    fn paid_for_sixty_percent_of_a_job_that_is_forty_percent_built() {
        // The failure the whole crate is for, in the contractors' own example.
        let p = project(
            &[("รวม", 1_000_000)],
            vec![
                phase("งวดที่ 1", 500_000, 500_000, 100.0),
                phase("งวดที่ 2", 500_000, 100_000, 0.0),
            ],
            vec![],
        );

        assert_eq!(p.drawn(), Money::from_baht(600_000));
        assert_eq!(p.earned(), Money::from_baht(500_000));
        assert_eq!(p.draw_ahead_points(), Some(10.0));

        let alerts = p.alerts();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].severity, Severity::Warning);
        assert!(
            alerts[0].message.contains("เบิกเงินนำงาน"),
            "{}",
            alerts[0].message
        );
    }

    #[test]
    fn drawing_far_ahead_of_the_work_is_critical_and_says_what_to_do() {
        let p = project(
            &[("รวม", 1_000_000)],
            vec![phase("งวดเดียว", 1_000_000, 700_000, 40.0)],
            vec![],
        );

        assert_eq!(p.draw_ahead_points(), Some(30.0));
        let alerts = p.alerts();
        assert_eq!(alerts[0].severity, Severity::Critical);
        assert!(
            alerts[0].message.contains("ก่อนจ่ายงวดถัดไป"),
            "an alert has to say what to do: {}",
            alerts[0].message
        );
    }

    #[test]
    fn work_running_ahead_of_the_money_is_not_a_problem() {
        // The contractor is financing the client. That is their business, and
        // not something to interrupt anyone about.
        let p = project(
            &[("รวม", 1_000_000)],
            vec![phase("งวดที่ 1", 1_000_000, 200_000, 80.0)],
            vec![],
        );

        assert_eq!(p.draw_ahead_points(), Some(-60.0));
        assert!(p.alerts().is_empty());
    }

    #[test]
    fn a_category_can_run_away_while_the_total_still_looks_fine() {
        // 560,000 of 1,000,000 overall — comfortable. But labour has spent
        // nearly twice what it was given, and by the time the total notices,
        // the money is gone.
        let p = project(
            &[("วัสดุ", 700_000), ("ค่าแรง", 300_000)],
            vec![],
            vec![cost("วัสดุ", 20_000, true), cost("ค่าแรง", 540_000, true)],
        );

        assert!(p.spend_percent().unwrap() < OVER_BUDGET_WARN);
        let over = p.categories_over_budget();
        assert_eq!(over.len(), 1);
        assert_eq!(over[0].0, "ค่าแรง");

        let alerts = p.alerts();
        assert_eq!(alerts.len(), 1);
        assert!(alerts[0].message.contains("ค่าแรง"), "{}", alerts[0].message);
    }

    #[test]
    fn the_worst_news_is_told_first() {
        let p = project(
            &[("วัสดุ", 100_000)],
            vec![phase("งวดเดียว", 1_000_000, 700_000, 40.0)],
            vec![cost("วัสดุ", 150_000, true), cost("วัสดุ", 10_000, false)],
        );

        let alerts = p.alerts();
        assert!(alerts.len() >= 3);
        assert_eq!(alerts[0].severity, Severity::Critical);
        assert!(
            alerts.windows(2).all(|w| w[0].severity >= w[1].severity),
            "alerts must be ordered worst first: {alerts:?}"
        );
    }

    #[test]
    fn a_project_with_no_budget_is_not_reported_as_over_it() {
        // Setting no budget is a legitimate state — the estimate has not been
        // entered yet. Dividing by it is not.
        let p = project(&[], vec![], vec![cost("วัสดุ", 50_000, true)]);
        assert_eq!(p.spend_percent(), None);
        assert!(p.alerts().iter().all(|a| !a.message.contains("ของงบ")));
    }

    #[test]
    fn a_phase_earns_in_proportion_to_what_is_built() {
        assert_eq!(phase("x", 1_000_000, 0, 0.0).earned(), Money::ZERO);
        assert_eq!(
            phase("x", 1_000_000, 0, 100.0).earned(),
            Money::from_baht(1_000_000)
        );
        assert_eq!(
            phase("x", 1_000_000, 0, 37.5).earned(),
            Money::from_baht(375_000)
        );
        // A completion someone typed as 120 is clamped, not trusted.
        assert_eq!(
            phase("x", 1_000_000, 0, 120.0).earned(),
            Money::from_baht(1_000_000)
        );
    }
}
