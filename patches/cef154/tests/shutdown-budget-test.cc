// Compile-time fake-clock regression against the production CEF source header.
#include "content/public/common/shutdown_timeouts.h"

namespace {
struct FakeClock {
  base::TimeDelta now;
  constexpr void Advance(base::TimeDelta duration) { now += duration; }
};

constexpr bool FullNetworkWaitLeavesRundownRoom(base::TimeDelta rundown) {
  FakeClock clock;
  const auto deadline = content::GetBrowserShutdownTimeout(rundown);
  clock.Advance(rundown / 2);  // Teardown before ProcessHostCleanUp.
  clock.Advance(content::GetNetworkServiceShutdownTimeout());
  if (clock.now >= deadline) return false;
  clock.Advance(rundown / 2 - base::Milliseconds(1));
  return clock.now < deadline;  // Cleanup and shutdown-complete signal still fit.
}
static_assert(FullNetworkWaitLeavesRundownRoom(base::Seconds(10)));
static_assert(FullNetworkWaitLeavesRundownRoom(base::Seconds(60)));

constexpr bool ForcedTerminationStillFits() {
  FakeClock clock;
  clock.Advance(base::Seconds(1));
  clock.Advance(content::GetNetworkServiceShutdownTimeout());
  clock.Advance(base::Seconds(2)); // SIGTERM, then forced kill and reap.
  clock.Advance(base::Seconds(8)); // Marker and remaining teardown work.
  return clock.now < content::GetBrowserShutdownTimeout(base::Seconds(10));
}
static_assert(ForcedTerminationStillFits());

#if !BUILDFLAG(IS_CHROMEOS)
constexpr bool OldEqualDeadlinesRace() {
  FakeClock clock;
  clock.Advance(base::Seconds(1));
  clock.Advance(content::GetNetworkServiceShutdownTimeout());
  return clock.now >= base::Seconds(10) &&
         clock.now < content::GetBrowserShutdownTimeout(base::Seconds(10));
}
static_assert(OldEqualDeadlinesRace());
#endif
}  // namespace
