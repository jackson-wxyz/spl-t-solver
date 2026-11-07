#!/usr/bin/env bash
# Job-queue worker for long solver runs.
#
# usage: tools/queue/runner.sh <worker-name> [threads]
#
# Jobs live in runs/queue/jobs/*.jobs, one shell command per line ('#' = comment). Files are
# processed in name order, so "10-experiment.jobs" runs before "50-overnight.jobs"; a file
# added while workers run is picked up at the next job. A job is claimed atomically with
# mkdir, so several workers can share the queue. Each job's output goes to
# runs/queue/logs/<id>.log; its RESULT / NEWBEST lines are appended to runs/queue/results.log.
# Create runs/queue/STOP to make workers exit after their current job.
set -u
cd "$(dirname "$0")/../.."
Q=runs/queue
name=${1:-w}
export RAYON_NUM_THREADS=${2:-6}
mkdir -p $Q/jobs $Q/claimed $Q/logs
echo "$(date '+%F %T') $name start (threads $RAYON_NUM_THREADS)" >> $Q/workers.log
while true; do
  [ -f $Q/STOP ] && { echo "$(date '+%F %T') $name stop" >> $Q/workers.log; exit 0; }
  found=0
  for f in $(ls $Q/jobs/*.jobs 2>/dev/null | sort); do
    while IFS= read -r line || [ -n "$line" ]; do
      line=${line%$'\r'}
      [ -z "${line// }" ] && continue
      case "$line" in \#*) continue ;; esac
      id=$(printf '%s' "$(basename $f):$line" | md5sum | cut -c1-12)
      if mkdir $Q/claimed/$id 2>/dev/null; then
        echo "$(basename $f) :: $line" > $Q/claimed/$id/job
        t0=$(date +%s)
        echo "$(date '+%F %T') $name begin $id $(basename $f) :: $line" >> $Q/workers.log
        bash -c "$line" > $Q/logs/$id.log 2>&1
        rc=$?
        t1=$(date +%s)
        grep -E "^(RESULT|NEWBEST|RATCHET)" $Q/logs/$id.log | sed "s|^|$(date '+%F %T') [$(basename $f)] |" >> $Q/results.log
        echo "$(date '+%F %T') $name end $id rc=$rc $((t1 - t0))s" >> $Q/workers.log
        found=1
        break 2
      fi
    done < "$f"
  done
  [ $found = 0 ] && sleep 20
done
