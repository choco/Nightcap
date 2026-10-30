import NightcapCore
import Testing

private struct Refused: Error {}

struct LoginItemAdapterTests {
    @Test func eachAttemptReadsAndReportsTheStatusMacOSLeftIncludingAfterFailure() {
        for refused in [false, true] {
            var events: [String] = []
            var status = LoginItem.notRegistered
            var reported: [LoginItem] = []
            let adapter = LoginItemAdapter(
                read: {
                    events.append("read")
                    return status
                },
                register: {
                    events.append("register")
                    if refused { throw Refused() }
                    status = .requiresApproval
                },
                unregister: {
                    events.append("unregister")
                    if refused { throw Refused() }
                    status = .notRegistered
                },
                onChange: {
                    events.append("report")
                    reported.append($0)
                })

            adapter.register()
            adapter.unregister()

            #expect(events == ["register", "read", "report", "unregister", "read", "report"])
            #expect(reported == (refused ? [.notRegistered, .notRegistered] : [.requiresApproval, .notRegistered]))
        }
    }

    @Test func pollingTheLoginItemDoesNotAnnounceAChange() {
        var reports = 0
        let adapter = LoginItemAdapter(
            read: { .enabled }, register: {}, unregister: {}, onChange: { _ in reports += 1 })
        #expect(adapter.item == .enabled)
        #expect(reports == 0)
    }
}
