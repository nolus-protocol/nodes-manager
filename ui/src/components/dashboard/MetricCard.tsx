import { ReactNode } from 'react';
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
  Skeleton,
  cn,
} from '@kostovster/ui';
import { TrendingUp, TrendingDown, Minus } from 'lucide-react';

interface MetricCardProps {
  title: string;
  value: string | number;
  subtitle?: string;
  icon: ReactNode;
  tooltip?: string;
  trend?: {
    value: number;
    label: string;
  };
  isLoading?: boolean;
  variant?: 'default' | 'success' | 'warning' | 'danger';
}

const variantStyles = {
  default: 'text-primary',
  success: 'text-green-500',
  warning: 'text-yellow-500',
  danger: 'text-red-500',
};

export function MetricCard({
  title,
  value,
  subtitle,
  icon,
  tooltip,
  trend,
  isLoading = false,
  variant = 'default',
}: MetricCardProps) {
  if (isLoading) {
    return (
      <Card>
        <CardHeader className="flex flex-row items-center justify-between pb-2">
          <Skeleton className="h-4 w-24" />
          <Skeleton className="h-5 w-5 rounded" />
        </CardHeader>
        <CardContent>
          <Skeleton className="h-9 w-20 mb-2" />
          <Skeleton className="h-4 w-32" />
        </CardContent>
      </Card>
    );
  }

  const TrendIcon = trend
    ? trend.value > 0
      ? TrendingUp
      : trend.value < 0
        ? TrendingDown
        : Minus
    : null;

  const trendColor = trend
    ? trend.value > 0
      ? 'text-green-500'
      : trend.value < 0
        ? 'text-red-500'
        : 'text-muted-foreground'
    : '';

  return (
    <Card className="hover:shadow-md transition-shadow">
      <CardHeader className="flex flex-row items-center justify-between pb-2">
        <CardTitle className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">
          {title}
        </CardTitle>
        {tooltip ? (
          <Tooltip>
            <TooltipTrigger asChild>
              <div className={cn('cursor-help', variantStyles[variant])}>{icon}</div>
            </TooltipTrigger>
            <TooltipContent>
              <p>{tooltip}</p>
            </TooltipContent>
          </Tooltip>
        ) : (
          <div className={variantStyles[variant]}>{icon}</div>
        )}
      </CardHeader>
      <CardContent>
        <div className="text-3xl font-bold tracking-tight">{value}</div>
        <div className="flex items-center justify-between mt-1">
          {subtitle && (
            <p className="text-sm text-muted-foreground">{subtitle}</p>
          )}
          {trend && TrendIcon && (
            <div className={cn('flex items-center gap-1 text-xs', trendColor)}>
              <TrendIcon className="h-3 w-3" />
              <span>{Math.abs(trend.value)}% {trend.label}</span>
            </div>
          )}
        </div>
      </CardContent>
    </Card>
  );
}
