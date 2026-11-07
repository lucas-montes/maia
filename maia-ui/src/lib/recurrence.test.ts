import { describe, expect, test } from "vitest";
import {
  addDaysMs,
  buildOccurrences,
  expandRecurrence,
  isOccurrenceOn,
  occurrenceIndex,
  parseRecurrence,
  type RecurrenceRule,
  type RecurrenceTaskLike,
} from "./recurrence";

// 2026-08-10 is a Monday — same anchor mobile's planner_recurrence_test.dart
// uses, so the cases below line up 1:1 with it.
const start = new Date(2026, 7, 10).getTime();
const day = (y: number, m: number, d: number) => new Date(y, m - 1, d).getTime();

describe("occurrenceIndex / isOccurrenceOn (port of PlannerRecurrence)", () => {
  test("daily with no end fires every day, anchor index 0", () => {
    const rule: RecurrenceRule = { type: "daily" };
    expect(occurrenceIndex(start, start, rule)).toBe(0);
    expect(isOccurrenceOn(start, start, rule)).toBe(true);
    expect(isOccurrenceOn(start, addDaysMs(start, 3), rule)).toBe(true);
    expect(isOccurrenceOn(start, addDaysMs(start, -1), rule)).toBe(false);
  });

  test("daily respects count (inclusive of anchor)", () => {
    const rule: RecurrenceRule = { type: "daily", count: 2 };
    expect(isOccurrenceOn(start, start, rule)).toBe(true); // index 0
    expect(isOccurrenceOn(start, addDaysMs(start, 1), rule)).toBe(true); // index 1
    expect(isOccurrenceOn(start, addDaysMs(start, 2), rule)).toBe(false); // index 2 >= count
  });

  test("daily respects endDate (inclusive)", () => {
    const rule: RecurrenceRule = { type: "daily", endDate: day(2026, 8, 11) };
    expect(isOccurrenceOn(start, day(2026, 8, 11), rule)).toBe(true);
    expect(isOccurrenceOn(start, day(2026, 8, 12), rule)).toBe(false);
  });

  test("weekly fires on selected weekdays only", () => {
    const rule: RecurrenceRule = { type: "weekly", weekdays: [3, 5] }; // Wed, Fri
    // Anchor is Monday, not a selected weekday.
    expect(isOccurrenceOn(start, start, rule)).toBe(false);
    const wed = day(2026, 8, 12);
    const fri = day(2026, 8, 14);
    const nextWed = day(2026, 8, 19);
    expect(occurrenceIndex(start, wed, rule)).toBe(0);
    expect(occurrenceIndex(start, fri, rule)).toBe(1);
    expect(occurrenceIndex(start, nextWed, rule)).toBe(2);
    expect(isOccurrenceOn(start, wed, rule)).toBe(true);
    expect(isOccurrenceOn(start, fri, rule)).toBe(true);
    expect(isOccurrenceOn(start, nextWed, rule)).toBe(true);
    expect(isOccurrenceOn(start, day(2026, 8, 11), rule)).toBe(false); // Tue
  });

  test("weekly respects count", () => {
    const rule: RecurrenceRule = { type: "weekly", weekdays: [3, 5], count: 2 };
    expect(isOccurrenceOn(start, day(2026, 8, 19), rule)).toBe(false); // idx 2
  });

  test("weekly respects endDate", () => {
    const rule: RecurrenceRule = { type: "weekly", weekdays: [3, 5], endDate: day(2026, 8, 14) };
    expect(isOccurrenceOn(start, day(2026, 8, 19), rule)).toBe(false);
    expect(isOccurrenceOn(start, day(2026, 8, 14), rule)).toBe(true);
  });

  test("interval fires every N days", () => {
    const rule: RecurrenceRule = { type: "interval", intervalDays: 2 };
    expect(occurrenceIndex(start, addDaysMs(start, 2), rule)).toBe(1);
    expect(isOccurrenceOn(start, addDaysMs(start, 1), rule)).toBe(false);
    expect(isOccurrenceOn(start, addDaysMs(start, 4), rule)).toBe(true);
  });

  test("interval respects count", () => {
    const rule: RecurrenceRule = { type: "interval", intervalDays: 2, count: 2 };
    expect(isOccurrenceOn(start, addDaysMs(start, 4), rule)).toBe(false); // index 2
  });

  test("interval respects endDate", () => {
    const rule: RecurrenceRule = { type: "interval", intervalDays: 2, endDate: day(2026, 8, 13) };
    expect(isOccurrenceOn(start, addDaysMs(start, 4), rule)).toBe(false);
  });

  test("monthly fires on the given month day", () => {
    const rule: RecurrenceRule = { type: "monthly", monthDay: 15 };
    expect(isOccurrenceOn(start, day(2026, 8, 15), rule)).toBe(true);
    expect(isOccurrenceOn(start, day(2026, 9, 15), rule)).toBe(true);
    expect(occurrenceIndex(start, day(2026, 9, 15), rule)).toBe(1);
    expect(isOccurrenceOn(start, day(2026, 8, 14), rule)).toBe(false);
  });

  test("monthly falls back to anchor day when monthDay is unset", () => {
    const rule: RecurrenceRule = { type: "monthly" };
    expect(isOccurrenceOn(start, day(2026, 9, 10), rule)).toBe(true);
    expect(occurrenceIndex(start, day(2026, 9, 10), rule)).toBe(1);
  });

  test("monthly respects count and endDate", () => {
    const countRule: RecurrenceRule = { type: "monthly", monthDay: 15, count: 1 };
    expect(isOccurrenceOn(start, day(2026, 9, 15), countRule)).toBe(false);
    const endRule: RecurrenceRule = { type: "monthly", monthDay: 15, endDate: day(2026, 8, 31) };
    expect(isOccurrenceOn(start, day(2026, 9, 15), endRule)).toBe(false);
  });
});

describe("parseRecurrence", () => {
  test("round-trips the JSON shape mobile writes", () => {
    const json = JSON.stringify({
      type: "weekly",
      weekdays: [2, 4],
      endDate: day(2026, 9, 1),
      count: 5,
      excludedDates: [day(2026, 8, 14)],
    });
    expect(parseRecurrence(json)).toEqual({
      type: "weekly",
      weekdays: [2, 4],
      endDate: day(2026, 9, 1),
      count: 5,
      excludedDates: [day(2026, 8, 14)],
    });
  });

  test("null/invalid input yields null, not a throw", () => {
    expect(parseRecurrence(null)).toBeNull();
    expect(parseRecurrence(undefined)).toBeNull();
    expect(parseRecurrence("")).toBeNull();
    expect(parseRecurrence("not json")).toBeNull();
    expect(parseRecurrence(JSON.stringify({ type: "yearly" }))).toBeNull();
  });
});

describe("expandRecurrence", () => {
  test("daily series expands to one entry per day in range", () => {
    const task: RecurrenceTaskLike = {
      id: "t1",
      date: start,
      title: "Stretch",
      done: 0,
      recurrence: JSON.stringify({ type: "daily" }),
    };
    const occ = expandRecurrence(task, start, addDaysMs(start, 4));
    expect(occ.map((o) => o.date)).toEqual([0, 1, 2, 3, 4].map((n) => addDaysMs(start, n)));
    expect(occ.every((o) => o.isVirtual)).toBe(true);
  });

  test("excluded date is skipped", () => {
    const skip = addDaysMs(start, 2);
    const task: RecurrenceTaskLike = {
      id: "t1",
      date: start,
      title: "Stretch",
      done: 0,
      recurrence: JSON.stringify({ type: "daily", excludedDates: [skip] }),
    };
    const occ = expandRecurrence(task, start, addDaysMs(start, 4));
    expect(occ.map((o) => o.date)).not.toContain(skip);
    expect(occ).toHaveLength(4);
  });

  test("bounded count/endDate stop expansion even with a wide range", () => {
    const task: RecurrenceTaskLike = {
      id: "t1",
      date: start,
      title: "Checkpoint",
      done: 0,
      recurrence: JSON.stringify({ type: "daily", count: 3 }),
    };
    const occ = expandRecurrence(task, start, addDaysMs(start, 30));
    expect(occ).toHaveLength(3);
    expect(occ[2]?.date).toBe(addDaysMs(start, 2));
  });

  test("weekly-by-weekday expands onto every matching weekday in a month view", () => {
    const task: RecurrenceTaskLike = {
      id: "t1",
      date: start, // Monday 2026-08-10
      title: "Gym",
      done: 0,
      recurrence: JSON.stringify({ type: "weekly", weekdays: [3, 5] }), // Wed, Fri
    };
    const occ = expandRecurrence(task, start, day(2026, 8, 31));
    expect(occ.map((o) => o.date)).toEqual(
      [12, 14, 19, 21, 26, 28].map((d) => day(2026, 8, d)),
    );
  });

  test("interval-every-N-days", () => {
    const task: RecurrenceTaskLike = {
      id: "t1",
      date: start,
      title: "Water plants",
      done: 0,
      recurrence: JSON.stringify({ type: "interval", intervalDays: 3 }),
    };
    const occ = expandRecurrence(task, start, addDaysMs(start, 9));
    expect(occ.map((o) => o.date)).toEqual([0, 3, 6, 9].map((n) => addDaysMs(start, n)));
  });

  test("monthly-by-day-of-month", () => {
    const task: RecurrenceTaskLike = {
      id: "t1",
      date: day(2026, 1, 15),
      title: "Pay rent",
      done: 0,
      recurrence: JSON.stringify({ type: "monthly", monthDay: 15 }),
    };
    const occ = expandRecurrence(task, day(2026, 1, 1), day(2026, 4, 30));
    expect(occ.map((o) => o.date)).toEqual(
      [[2026, 1, 15], [2026, 2, 15], [2026, 3, 15], [2026, 4, 15]].map(([y, m, d]) => day(y, m, d)),
    );
  });

  test("non-recurring task expands to nothing", () => {
    const task: RecurrenceTaskLike = { id: "t1", date: start, title: "One-off", done: 0, recurrence: null };
    expect(expandRecurrence(task, start, addDaysMs(start, 30))).toEqual([]);
  });
});

describe("buildOccurrences", () => {
  test("a one-off task shows exactly once on its due date", () => {
    const due = day(2026, 8, 20);
    const tasks: RecurrenceTaskLike[] = [
      { id: "a", date: start, title: "Submit report", done: 0, dueDate: due, recurrence: null },
    ];
    const occ = buildOccurrences(tasks, day(2026, 8, 1), day(2026, 8, 31));
    expect(occ).toHaveLength(1);
    expect(occ[0]).toMatchObject({ id: "a", date: due, isVirtual: false });
  });

  test("a recurring weekly task shows up on every matching weekday across the visible month", () => {
    const tasks: RecurrenceTaskLike[] = [
      {
        id: "series-anchor",
        date: start, // Monday 2026-08-10
        title: "Standup",
        done: 0,
        seriesId: "series-anchor",
        recurrence: JSON.stringify({ type: "weekly", weekdays: [1, 3, 5] }), // Mon/Wed/Fri
      },
    ];
    const occ = buildOccurrences(tasks, day(2026, 8, 1), day(2026, 8, 31));
    const dates = occ.map((o) => o.date);
    // Mon/Wed/Fri from the 10th through the 31st, including the anchor's own day.
    expect(dates).toEqual(
      [10, 12, 14, 17, 19, 21, 24, 26, 28, 31].map((d) => day(2026, 8, d)),
    );
    // The anchor's own day is the real row, not a synthesized one.
    expect(occ.find((o) => o.date === start)).toMatchObject({ id: "series-anchor", isVirtual: false });
    expect(occ.find((o) => o.date === day(2026, 8, 12))).toMatchObject({ isVirtual: true });
  });

  test("an already-materialized occurrence wins over the synthesized one for the same day", () => {
    const tasks: RecurrenceTaskLike[] = [
      {
        id: "anchor",
        date: start,
        title: "Standup",
        done: 0,
        seriesId: "anchor",
        recurrence: JSON.stringify({ type: "daily" }),
      },
      {
        // Mobile already materialized + edited this occurrence (different id, same series+day).
        id: "materialized-1",
        date: addDaysMs(start, 1),
        title: "Standup (moved to afternoon)",
        done: 1,
        seriesId: "anchor",
        recurrence: null,
      },
    ];
    const occ = buildOccurrences(tasks, start, addDaysMs(start, 2));
    expect(occ).toHaveLength(3);
    const dayOne = occ.find((o) => o.date === addDaysMs(start, 1));
    expect(dayOne).toMatchObject({ id: "materialized-1", title: "Standup (moved to afternoon)", done: true, isVirtual: false });
  });
});
