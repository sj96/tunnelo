import type { MappingTrafficSnapshot } from "../types";
import { formatRate } from "../formatTraffic";

export interface TrafficPoint {
  ts: number;
  up: number;
  down: number;
}

interface Props {
  snapshot?: MappingTrafficSnapshot;
  history: TrafficPoint[];
  aggregate?: boolean;
  aggregateRates?: { down: number; up: number };
}

function pointsPath(points: TrafficPoint[], key: "up" | "down", max: number): string {
  if (points.length < 2) return "";
  return points
    .map((point, index) => {
      const x = (index / (points.length - 1)) * 100;
      const y = 22 - (point[key] / max) * 18;
      return `${index === 0 ? "M" : "L"} ${x.toFixed(2)} ${y.toFixed(2)}`;
    })
    .join(" ");
}

export function TunnelMonitor({ snapshot, history, aggregate = false, aggregateRates }: Props) {
  const max = Math.max(1, ...history.flatMap((point) => [point.up, point.down]));
  const downRate = aggregateRates?.down ?? snapshot?.bytesDownPerSec;
  const upRate = aggregateRates?.up ?? snapshot?.bytesUpPerSec;

  return (
    <div className={aggregate ? "aggregate-traffic-monitor" : "forward-traffic-monitor"}>
      <div
        className={aggregate ? "aggregate-traffic-chart" : "forward-item-chart"}
        aria-label={aggregate ? "Aggregate network traffic chart" : "Network traffic chart"}
      >
        <svg viewBox="0 0 100 24" preserveAspectRatio="none" role="img">
          {history.length > 1 && (
            <>
              <path className="monitor-line down" d={pointsPath(history, "down", max)} />
              <path className="monitor-line up" d={pointsPath(history, "up", max)} />
            </>
          )}
        </svg>
      </div>
      <div
        className={aggregate ? "aggregate-rate-stack" : "forward-rate-stack"}
        aria-label="Traffic rates"
      >
        <span className="forward-rate-line forward-rate-down" title="Download rate">
          <span aria-hidden>↓</span>
          {downRate !== undefined ? formatRate(downRate) : "—"}
        </span>
        <span className="forward-rate-line forward-rate-up" title="Upload rate">
          <span aria-hidden>↑</span>
          {upRate !== undefined ? formatRate(upRate) : "—"}
        </span>
      </div>
    </div>
  );
}
