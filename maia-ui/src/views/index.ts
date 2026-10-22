import type { ViewId } from "../lib/stores";
import type { ViewModule } from "../router";

import * as dashboard from "./dashboard";
import * as todos from "./todos";
import * as goals from "./goals";
import * as notes from "./notes";
import * as urls from "./urls";
import * as settings from "./settings";
import * as receipts from "./receipts";
import * as exercises from "./exercises";
import * as workouts from "./workouts";
import * as templates from "./templates";
import * as ingredients from "./ingredients";
import * as foods from "./foods";
import * as stores from "./stores";
import * as meals from "./meals";
import * as bodyMetrics from "./bodyMetrics";
import * as experiments from "./experiments";
import * as tags from "./tags";
import * as accounts from "./accounts";
import * as transactions from "./transactions";
import * as fxRates from "./fxRates";
import * as sync from "./sync";

export const routes: Record<ViewId, ViewModule> = {
  dashboard,
  todos,
  goals,
  notes,
  urls,
  settings,
  receipts,
  exercises,
  workouts,
  templates,
  ingredients,
  foods,
  stores,
  meals,
  bodyMetrics,
  experiments,
  tags,
  accounts,
  transactions,
  fxRates,
  sync,
};
