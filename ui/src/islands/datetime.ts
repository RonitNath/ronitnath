const pad2 = (value: number) => String(value).padStart(2, "0");

export function rfc3339ToDateValue(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  return [
    date.getFullYear(),
    "-",
    pad2(date.getMonth() + 1),
    "-",
    pad2(date.getDate()),
  ].join("");
}

export function rfc3339ToTimeValue(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  return [pad2(date.getHours()), ":", pad2(date.getMinutes())].join("");
}

export function pickerPartsToRfc3339(dateValue: string, timeValue: string): string {
  if (!dateValue || !timeValue) return "";
  const date = new Date(`${dateValue}T${timeValue}`);
  if (Number.isNaN(date.getTime())) return "";
  const offsetMinutes = -date.getTimezoneOffset();
  const sign = offsetMinutes >= 0 ? "+" : "-";
  const absOffset = Math.abs(offsetMinutes);
  return [
    date.getFullYear(),
    "-",
    pad2(date.getMonth() + 1),
    "-",
    pad2(date.getDate()),
    "T",
    pad2(date.getHours()),
    ":",
    pad2(date.getMinutes()),
    ":00",
    sign,
    pad2(Math.floor(absOffset / 60)),
    ":",
    pad2(absOffset % 60),
  ].join("");
}

export function updateDatePart(currentValue: string, dateValue: string, fallbackTime: string): string {
  return pickerPartsToRfc3339(dateValue, rfc3339ToTimeValue(currentValue) || fallbackTime);
}

export function updateTimePart(currentValue: string, timeValue: string): string {
  const dateValue = rfc3339ToDateValue(currentValue);
  if (!dateValue) return currentValue;
  return pickerPartsToRfc3339(dateValue, timeValue);
}
