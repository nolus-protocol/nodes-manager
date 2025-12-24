interface ParsedCron {
  second: string;
  minute: string;
  hour: string;
  dayOfMonth: string;
  month: string;
  dayOfWeek: string;
}

export function parseCron(expression: string): ParsedCron | null {
  const parts = expression.trim().split(/\s+/);
  if (parts.length !== 6) return null;
  
  return {
    second: parts[0],
    minute: parts[1],
    hour: parts[2],
    dayOfMonth: parts[3],
    month: parts[4],
    dayOfWeek: parts[5],
  };
}

export function getNextRun(expression: string): Date | null {
  const parsed = parseCron(expression);
  if (!parsed) return null;

  const now = new Date();
  const next = new Date(now);

  // Set seconds
  next.setUTCSeconds(parsed.second !== '*' ? (parseInt(parsed.second) || 0) : 0, 0);

  // Set minutes
  if (parsed.minute !== '*') {
    const targetMinute = parseInt(parsed.minute);
    if (targetMinute >= 0 && targetMinute <= 59) {
      next.setUTCMinutes(targetMinute);
    }
  }

  // Set hours
  if (parsed.hour !== '*') {
    const targetHour = parseInt(parsed.hour);
    if (targetHour >= 0 && targetHour <= 23) {
      next.setUTCHours(targetHour);
    }
  }

  // Handle day of week (standard cron format: 0=Sunday, 1=Monday, ..., 6=Saturday, 7=Sunday)
  if (parsed.dayOfWeek !== '*') {
    const targetDayNum = parseInt(parsed.dayOfWeek);
    const targetJSDay = targetDayNum === 7 ? 0 : targetDayNum;
    const currentJSDay = next.getUTCDay();
    let daysToAdd = targetJSDay - currentJSDay;

    if (daysToAdd < 0 || (daysToAdd === 0 && next <= now)) {
      daysToAdd += 7;
    }

    if (daysToAdd > 0) {
      next.setUTCDate(next.getUTCDate() + daysToAdd);
    }
  } else if (next <= now) {
    next.setUTCDate(next.getUTCDate() + 1);
  }

  return next;
}

export function formatNextRun(cronSchedule: string | undefined): string | null {
  if (!cronSchedule) return null;
  
  const nextRun = getNextRun(cronSchedule);
  if (!nextRun) return null;

  const timeString = nextRun.toLocaleTimeString([], { 
    hour: '2-digit', 
    minute: '2-digit', 
    hour12: true 
  });
  const dayString = nextRun.toLocaleDateString([], { weekday: 'long' });
  const dateString = nextRun.toLocaleDateString([], { 
    day: '2-digit', 
    month: 'short' 
  });

  return `${timeString}, ${dayString}, ${dateString}`;
}

export function getCronSortValue(cronSchedule: string | undefined): number {
  if (!cronSchedule) return 99999;
  const parts = cronSchedule.trim().split(/\s+/);
  if (parts.length !== 6) return 99999;
  const hour = parseInt(parts[2]) || 0;
  const dayOfWeek = parseInt(parts[5]) || 0;
  return dayOfWeek * 100 + hour;
}
