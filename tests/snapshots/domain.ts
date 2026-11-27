import { Schema, Data, Match } from "effect"

export const UserId = Schema.String.pipe(Schema.brand("UserId"))
export type UserId = typeof UserId.Type

export const User = Schema.Struct({
  id: UserId,
  name: Schema.String,
  email: Schema.optional(Schema.String),
  roles: Schema.Array(Schema.Literals(["admin", "member"])),
  manager: Schema.NullOr(UserId)
})
export type User = typeof User.Type

export class Account extends Schema.Class<Account, { readonly brand: unique symbol }>("Account")({
  owner: UserId,
  balance: Schema.Finite
}) {}

export const Deposited = Schema.TaggedStruct("Deposited", {
  amount: Schema.Finite
})
export type Deposited = typeof Deposited.Type

export const Withdrew = Schema.TaggedStruct("Withdrew", {
  amount: Schema.Finite
})
export type Withdrew = typeof Withdrew.Type

export const LedgerEntry = Schema.Union([Deposited, Withdrew])
export type LedgerEntry = typeof LedgerEntry.Type

export class UserNotFound extends Data.TaggedError("UserNotFound")<{
  readonly id: UserId
}> {}

export class InsufficientFunds extends Data.TaggedError("InsufficientFunds")<{
  readonly needed: number
  readonly available: number
}> {}

export class Unauthorized extends Data.TaggedError("Unauthorized")<{}> {}

export type Shape = Data.TaggedEnum<{
  Circle: { readonly radius: number }
  Square: { readonly side: number }
  Point: {}
}>
export const Shape = Data.taggedEnum<Shape>()

export function area(shape: Shape): number {
  return Match.valueTags(shape, {
    Circle: ({ radius }) => Math.PI * radius ** 2,
    Square: ({ side }) => side * side,
    Point: () => 0
  })
}

export function describe(entry: LedgerEntry): string {
  return Match.valueTags(entry, {
    Deposited: (d) => `+${d.amount}`,
    Withdrew: (w) => `-${w.amount}`
  })
}
