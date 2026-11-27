import { Data, Schema, Match } from "effect"

export const UserId = Schema.String.pipe(Schema.brand("UserId"))

export const User = Schema.Struct({
  id: UserId,
  name: Schema.String,
  email: Schema.optional(Schema.String),
  roles: Schema.Array(Schema.Literals(["admin", "member"])),
  manager: Schema.NullOr(UserId)
})

export class Account extends Schema.Class("Account")({
  owner: UserId,
  balance: Schema.Finite
}) {}

export const Deposited = Schema.TaggedStruct("Deposited", {
  amount: Schema.Finite
})

export const Withdrew = Schema.TaggedStruct("Withdrew", {
  amount: Schema.Finite
})

export const LedgerEntry = Schema.Union([Deposited, Withdrew])

export class UserNotFound extends Data.TaggedError("UserNotFound") {}

export class InsufficientFunds extends Data.TaggedError("InsufficientFunds") {}

export class Unauthorized extends Data.TaggedError("Unauthorized") {}

export const Shape = Data.taggedEnum()

export const area = (shape) => Match.valueTags(shape, {
  Circle: ({ radius }) => Math.PI * radius ** 2,
  Square: ({ side }) => side * side,
  Point: () => 0
})

export const describe = (entry) => Match.valueTags(entry, {
  Deposited: (d) => `+${d.amount}`,
  Withdrew: (w) => `-${w.amount}`
})
