import IdentityLookup

/// Runs live when a message arrives, so every rule type works here with no
/// expansion. The strongest verdict available is the Junk folder — messages
/// cannot be deleted or stopped — and only unknown senders are offered.
///
/// Real filtering arrives with F004. This proves the extension links the core.
class MessageFilterExtension: ILMessageFilterExtension {}

extension MessageFilterExtension: ILMessageFilterQueryHandling {
    func handle(_ queryRequest: ILMessageFilterQueryRequest,
                context: ILMessageFilterExtensionContext,
                completion: @escaping (ILMessageFilterQueryResponse) -> Void) {
        _ = coreVersion()
        completion(ILMessageFilterQueryResponse())
    }
}
