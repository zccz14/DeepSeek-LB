export type UserTableSortRecord = {
  id: string
  role: string
  allow_debt: boolean
  created_at: number
  topup_usd_nanos: number
  consumed_usd_nanos: number
  provided_usd_nanos: number
  available_usd_nanos: number
}

const userTableSortValues = {
  created_at: (user: UserTableSortRecord) => user.created_at,
  topup_usd_nanos: (user: UserTableSortRecord) => user.topup_usd_nanos,
  consumed_usd_nanos: (user: UserTableSortRecord) => user.consumed_usd_nanos,
  provided_usd_nanos: (user: UserTableSortRecord) => user.provided_usd_nanos,
  available_usd_nanos: (user: UserTableSortRecord) => user.available_usd_nanos,
  role: (user: UserTableSortRecord) => user.role,
  allow_debt: (user: UserTableSortRecord) => Number(user.allow_debt),
}

export type UserTableSortColumn = keyof typeof userTableSortValues

export function userTableSortValue(
  user: UserTableSortRecord,
  column: UserTableSortColumn
) {
  return userTableSortValues[column](user)
}

export function isUserTableNumericColumn(column: string) {
  return column.endsWith("_usd_nanos")
}
