import { Effect, Option, pipe, Match } from "effect"

export function clamp(value, min = 0, max = 100) {
  if (value < min) {
    return min
  } else if (value > max) {
    return max
  }
  return value
}

function sum(...xs) {
  return xs.reduce((a, b) => a + b, 0)
}

function identity(value) {
  return value
}

export function classify(n) {
  return (n === 0 ? "zero" : (n < 0 ? "negative" : "positive"))
}

export function describe(code) {
  return Match.value(code).pipe(
    Match.when(200, () => "ok"),
    Match.when("teapot", () => "short and stout"),
    Match.orElse(() => `unknown: ${code}`)
  )
}

function shapes() {
  const { x, y: why = 1, ...rest } = { x: 1, y: 2, z: 3 }
  const [first, , third = 3, ...others] = [1, 2, 3, 4]
  let total = 0
  for (const n of [1, 2, 3]) {
    total += n
  }
  while (total > 0) {
    total--
    if (total === 2) {
      break
    }
  }
  const point = { x, why, rest, first, third, others, [`k${total}`]: total, "quoted": true }
  const nested = point?.rest?.["z"] ?? 0
  const casted = (nested)
  const maybe = document.getElementById("app")
  const map = new Map([["a", 1]])
  const typed = identity(1)
  const neg = -casted + +typed
  const flags = !maybe && typeof map === "object"
  const pow = 2 ** 3 ** 2
  const shifted = (1 << 3) >> 1
  const arrows = [(a) => a * 2, () => ({ ok: true }), (x) => {
    return x
  }]
  return { point, nested, map, neg, flags, pow, shifted, arrows }
}

export const piped = pipe(5, ((n) => n + 1), String)

export const optionValue = pipe(Option.some(1), Option.map((n) => n + 1))

export const delayed = Effect.sleep("1 second")
