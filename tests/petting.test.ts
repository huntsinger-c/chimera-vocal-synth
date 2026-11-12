/**
 * Petting a head: a short wiggle over the crown makes it happy (eyes closing, hearts), a single
 * pass doesn't, and it settles a few seconds after the petting stops.
 */
import { describe, expect, test } from "bun:test";
import { insideOutline, Petting, Stroking } from "../src/ui/render/petting.ts";
import { CREATURE_VISUALS } from "../src/ui/creatureVisuals.ts";

/** wiggle at `cmPerS` (head cm per second) for `s` seconds, at 60 frames a second */
function wiggle(p: Petting, cmPerS: number, s: number) {
  for (let t = 0; t < s; t += 1 / 60) {
    p.stroke(cmPerS / 60);
    p.update(1 / 60, [100, 100], 20);
  }
}
const rest = (p: Petting, s: number) => {
  for (let t = 0; t < s; t += 1 / 60) p.update(1 / 60, [100, 100], 20);
};

describe("Petting", () => {
  test("a second and a half of ordinary wiggling makes it happy: eyes close, hearts float up", () => {
    const p = new Petting();
    wiggle(p, 30, 1.6);
    expect(p.happy).toBe(true);
    rest(p, 0.6);
    expect(p.content).toBeGreaterThan(0.9);
    expect(p.hearts.length).toBeGreaterThan(0);
  });

  test("passing over the head once does nothing", () => {
    const p = new Petting();
    wiggle(p, 30, 0.4); // 12 cm: across the crown and off
    rest(p, 1);
    expect(p.happy).toBe(false);
    expect(p.content).toBeLessThan(0.01);
    expect(p.hearts.length).toBe(0);
  });

  test("it settles a few seconds after the petting stops, and the hearts finish", () => {
    const p = new Petting();
    wiggle(p, 40, 4);
    expect(p.happy).toBe(true);
    rest(p, 2);
    expect(p.happy).toBe(true); // it lingers
    rest(p, 4);
    expect(p.happy).toBe(false);
    rest(p, 2);
    expect(p.content).toBeLessThan(0.01);
    expect(p.hearts.length).toBe(0);
  });
});

describe("Stroking the cat", () => {
  const cat = CREATURE_VISUALS.cat;
  /** stroke along the back and return, `passes` times, at 60 moves a second */
  function stroke(s: Stroking, passes: number, from: [number, number] = [-8, 14], to: [number, number] = [8, 26]) {
    for (let k = 0; k < passes; k++) {
      for (let i = 0; i <= 20; i++) {
        const u = (k % 2 ? 20 - i : i) / 20;
        s.move([from[0] + (to[0] - from[0]) * u, from[1] + (to[1] - from[1]) * u], 6);
        s.pet.update(1 / 60, [0, 0], 20);
      }
    }
  }

  test("its outline holds its body, its head and its tail, and nothing beside them", () => {
    for (const p of [[0, 12], [17, 34], [14, 5], [-26, 2], [10, 20]] as [number, number][]) expect(insideOutline(p, cat.silhouette)).toBe(true);
    // in front of its chest, above its back, between its ears, under the floor, behind its tail
    for (const p of [[22, 15], [-8, 32], [12.4, 45.5], [0, -2], [-37, 5]] as [number, number][]) expect(insideOutline(p, cat.silhouette)).toBe(false);
    // what it is drawn with is on it: the eye, the larynx, and the airway (the mouth and nose tubes end at the lips and nostrils, on the outline)
    for (const part of cat.parts ?? []) if (part.eye) for (const p of part.pts) expect(insideOutline(p, cat.silhouette)).toBe(true);
    for (const src of cat.sources ?? []) expect(insideOutline(src.at, cat.silhouette)).toBe(true);
    for (const t of cat.tubes) expect(insideOutline(t.path[0], cat.silhouette)).toBe(true);
    // and the hearts start above its head
    expect(insideOutline(cat.pet!.hearts, cat.silhouette)).toBe(false);
  });

  test("four strokes along its back make it happy; one doesn't, and the same path beside it does nothing", () => {
    const once = new Stroking(cat.silhouette);
    stroke(once, 1);
    expect(once.pet.happy).toBe(false); // 20 cm: a hand passing over
    const s = new Stroking(cat.silhouette);
    stroke(s, 4); // there and back twice, 1.4 s
    expect(s.pet.happy).toBe(true);
    const beside = new Stroking(cat.silhouette);
    stroke(beside, 6, [-30, 30], [-10, 45]);
    expect(beside.pet.level).toBe(0);
  });

  test("a path that leaves the body and comes back counts only what was on it", () => {
    const s = new Stroking(cat.silhouette);
    s.move([0, 12]);
    s.move([0, -30]); // off, below the floor
    s.move([0, 14]); // back on: no jump across the gap
    expect(s.pet.level).toBe(0);
    s.move([0, 20]);
    expect(s.pet.level).toBeCloseTo((6 * 0.6) / 30, 6);
  });

  test("a press that lifts where it landed is a tap; one that moved was a stroke; off the body it is neither", () => {
    const s = new Stroking(cat.silhouette);
    expect(s.press([0, 12])).toBe(true);
    s.move([0.1, 12], 2);
    expect(s.lift(0)).toBe("tap");
    s.press([0, 12]);
    s.move([4, 14], 40);
    expect(s.lift(100)).toBe(null);
    expect(s.press([22, 15])).toBe(false);
    expect(s.lift(200)).toBe(null);
    // a lift with no press (the press landed on another screen) is nothing either
    expect(s.lift(300)).toBe(null);
  });

  test("the fifth tap in a row annoys it and ends its good mood; taps spread out never do", () => {
    const s = new Stroking(cat.silhouette);
    stroke(s, 4);
    expect(s.pet.happy).toBe(true);
    const tap = (now: number) => {
      s.press([0, 12]);
      return s.lift(now);
    };
    expect([0, 300, 600, 900].map(tap)).toEqual(["tap", "tap", "tap", "tap"]);
    expect(tap(1200)).toBe("annoyed");
    expect(s.pet.happy).toBe(false);
    expect(s.pet.level).toBe(0);
    // it forgives: the count starts again
    expect(tap(1500)).toBe("tap");
    const calm = new Stroking(cat.silhouette);
    const spread = [0, 1000, 2000, 3000, 4000, 5000, 6000].map((now) => {
      calm.press([0, 12]);
      return calm.lift(now);
    });
    expect(spread.every((x) => x === "tap")).toBe(true);
  });

  test("a finger that lifts is gone: the next touch does not stroke across the gap", () => {
    const s = new Stroking(cat.silhouette);
    s.press([-8, 14]);
    s.move([0, 20], 80);
    const level = s.pet.level;
    expect(level).toBeGreaterThan(0);
    s.lift(0, true);
    s.press([14, 5]);
    s.move([14, 5.5], 1);
    expect(s.pet.level).toBeCloseTo(level + (0.5 * 0.6) / 30, 6);
  });
});
