import AppIntents
import WidgetKit

/// An account choice shown in the widget's configuration (long-press → Edit).
/// The list is populated from the snapshot the app publishes (names only — no
/// figures).
struct AccountEntity: AppEntity {
  let id: String
  let name: String

  static var typeDisplayRepresentation: TypeDisplayRepresentation = "Account"
  var displayRepresentation: DisplayRepresentation { DisplayRepresentation(title: "\(name)") }

  static var defaultQuery = AccountQuery()
}

struct AccountQuery: EntityQuery {
  func entities(for identifiers: [String]) async throws -> [AccountEntity] {
    SharedHealth.load()
      .filter { identifiers.contains($0.id) }
      .map { AccountEntity(id: $0.id, name: $0.name) }
  }

  func suggestedEntities() async throws -> [AccountEntity] {
    SharedHealth.load().map { AccountEntity(id: $0.id, name: $0.name) }
  }
}

/// The per-widget configuration: which account this instance tracks.
struct SelectAccountIntent: WidgetConfigurationIntent {
  static var title: LocalizedStringResource = "Select Account"
  static var description = IntentDescription("Choose which account this widget tracks.")

  @Parameter(title: "Account")
  var account: AccountEntity?
}
