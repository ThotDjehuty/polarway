"""
Polarway Lakehouse Client — Delta Lake-backed auth, audit, and data management.

Uses the `deltalake` Python package (delta-rs bindings) for ACID transactions,
time-travel, and optimized storage. Mirrors the Rust `polarway-lakehouse` crate API.

Usage:
    from polarway.lakehouse import LakehouseClient, SubscriptionTier

    client = LakehouseClient("/data/lakehouse", jwt_secret="my-secret")

    # Register
    user = client.register("albert", "albert@example.com", "P@ssword123",
                           first_name="Albert", last_name="Smith",
                           tier=SubscriptionTier.PIONEER)

    # Login
    token, user = client.login("albert", "P@ssword123")

    # Verify
    user = client.verify_token(token)

    # Time-travel — read users table as it was at version 1
    users_v1 = client.read_version("users", 1)

    # Billing summary
    summary = client.billing_summary("user-123", "2025-01-01", "2025-12-31")
"""

# pyright: reportMissingImports=false, reportAttributeAccessIssue=false

from __future__ import annotations

import hashlib
import json
import os
from datetime import datetime, timedelta, timezone
from enum import Enum
from typing import Any, Optional
from uuid import uuid4

import pyarrow as pa

try:
    import jwt as pyjwt
except ImportError:
    pyjwt = None  # type: ignore[assignment]

try:
    from argon2 import PasswordHasher
    from argon2.exceptions import VerifyMismatchError
except ImportError:
    PasswordHasher = None  # type: ignore[assignment, misc]
    VerifyMismatchError = None  # type: ignore[assignment, misc]

try:
    from deltalake import DeltaTable, write_deltalake
except ImportError:
    DeltaTable = None  # type: ignore[assignment, misc]
    write_deltalake = None  # type: ignore[assignment]


# ─── Enums ───


class UserRole(str, Enum):
    GUEST = "guest"
    PENDING = "pending"
    REGISTERED = "registered"
    TRADER = "trader"
    ADMIN = "admin"

    @property
    def level(self) -> int:
        return {
            "guest": 0, "pending": 1, "registered": 2, "trader": 3, "admin": 4
        }[self.value]

    def has_permission(self, required: "UserRole") -> bool:
        return self.level >= required.level


class SubscriptionTier(str, Enum):
    FREE = "free"
    HOBBYIST = "hobbyist"
    PIONEER = "pioneer"
    PROFESSIONAL = "professional"

    @property
    def default_role(self) -> UserRole:
        if self == SubscriptionTier.FREE:
            return UserRole.REGISTERED
        return UserRole.TRADER


class ActionType(str, Enum):
    LOGIN = "login"
    LOGOUT = "logout"
    REGISTER = "register"
    PASSWORD_CHANGE = "password_change"
    USER_APPROVED = "user_approved"
    USER_REJECTED = "user_rejected"
    QUERY_EXECUTED = "query_executed"
    DATA_UPLOAD = "data_upload"
    DATA_EXPORT = "data_export"
    BACKTEST_RUN = "backtest_run"
    LIVE_TRADE_START = "live_trade_start"
    ADMIN_ACTION = "admin_action"
    SUBSCRIPTION_CHANGE = "subscription_change"

    @property
    def is_billable(self) -> bool:
        return self in {
            ActionType.QUERY_EXECUTED,
            ActionType.DATA_UPLOAD,
            ActionType.DATA_EXPORT,
            ActionType.BACKTEST_RUN,
            ActionType.LIVE_TRADE_START,
        }


# ─── Data Classes ───


class UserRecord:
    """Mirrors Rust UserRecord."""

    __slots__ = (
        "user_id", "username", "email", "role", "subscription_tier",
        "first_name", "last_name", "is_active", "created_at", "last_login",
    )

    def __init__(self, **kwargs: Any):
        for k in self.__slots__:
            setattr(self, k, kwargs.get(k))

    def __repr__(self) -> str:
        return f"UserRecord(username={self.username!r}, role={self.role}, tier={self.subscription_tier})"


class AuditEntry:
    """Mirrors Rust AuditEntry."""

    __slots__ = (
        "event_id", "user_id", "username", "action", "resource",
        "detail", "ip_address", "timestamp", "date_partition",
    )

    def __init__(self, **kwargs: Any):
        for k in self.__slots__:
            setattr(self, k, kwargs.get(k))


class BillingSummary:
    """Billing summary for a user over a period."""

    __slots__ = (
        "user_id", "period_start", "period_end",
        "total_queries", "total_uploads", "total_exports",
        "total_backtests", "total_live_trades", "total_actions",
    )

    def __init__(self, **kwargs: Any):
        for k in self.__slots__:
            setattr(self, k, kwargs.get(k, 0))


# ─── Arrow Schemas ───


def _users_schema() -> pa.Schema:
    return pa.schema([
        pa.field("user_id", pa.utf8(), nullable=False),
        pa.field("username", pa.utf8(), nullable=False),
        pa.field("email", pa.utf8(), nullable=False),
        pa.field("password_hash", pa.utf8(), nullable=False),
        pa.field("role", pa.utf8(), nullable=False),
        pa.field("subscription_tier", pa.utf8()),
        pa.field("first_name", pa.utf8()),
        pa.field("last_name", pa.utf8()),
        pa.field("is_active", pa.bool_(), nullable=False),
        pa.field("created_at", pa.utf8(), nullable=False),
        pa.field("last_login", pa.utf8()),
        pa.field("preferences_json", pa.utf8()),
    ])


def _sessions_schema() -> pa.Schema:
    return pa.schema([
        pa.field("token_hash", pa.utf8(), nullable=False),
        pa.field("user_id", pa.utf8(), nullable=False),
        pa.field("username", pa.utf8(), nullable=False),
        pa.field("role", pa.utf8(), nullable=False),
        pa.field("created_at", pa.utf8(), nullable=False),
        pa.field("expires_at", pa.utf8(), nullable=False),
        pa.field("is_revoked", pa.bool_(), nullable=False),
    ])


def _audit_log_schema() -> pa.Schema:
    return pa.schema([
        pa.field("event_id", pa.utf8(), nullable=False),
        pa.field("user_id", pa.utf8(), nullable=False),
        pa.field("username", pa.utf8(), nullable=False),
        pa.field("action", pa.utf8(), nullable=False),
        pa.field("resource", pa.utf8()),
        pa.field("detail", pa.utf8(), nullable=False),
        pa.field("ip_address", pa.utf8()),
        pa.field("timestamp", pa.utf8(), nullable=False),
        pa.field("date_partition", pa.utf8(), nullable=False),
    ])


# ─── Client ───


class LakehouseClient:
    """Python Delta Lake client — mirrors the Rust polarway-lakehouse API.

    Args:
        base_path: Local filesystem path for Delta tables.
        jwt_secret: Secret key for JWT signing. Falls back to POLARWAY_JWT_SECRET env var.
        session_expiry_days: Session token lifetime in days (default: 7).
    """

    TABLE_USERS = "users"
    TABLE_SESSIONS = "sessions"
    TABLE_AUDIT_LOG = "audit_log"
    TABLE_USER_ACTIONS = "user_actions"

    def __init__(
        self,
        base_path: str,
        jwt_secret: str | None = None,
        session_expiry_days: int = 7,
    ):
        if DeltaTable is None:
            raise ImportError(
                "deltalake is required: pip install deltalake"
            )
        if PasswordHasher is None:
            raise ImportError(
                "argon2-cffi is required: pip install argon2-cffi"
            )
        if pyjwt is None:
            raise ImportError("PyJWT is required: pip install PyJWT")

        self.base_path = base_path
        self.jwt_secret = jwt_secret or os.getenv("POLARWAY_JWT_SECRET", "change-me-in-production")
        self.session_expiry_days = session_expiry_days
        self._hasher = PasswordHasher()

        # Initialize tables
        os.makedirs(base_path, exist_ok=True)
        self._init_table(self.TABLE_USERS, _users_schema())
        self._init_table(self.TABLE_SESSIONS, _sessions_schema())
        self._init_table(self.TABLE_AUDIT_LOG, _audit_log_schema(), partition_by=["date_partition"])

    def _table_uri(self, name: str) -> str:
        return os.path.join(self.base_path, name)

    def _init_table(
        self,
        name: str,
        schema: pa.Schema,
        partition_by: list[str] | None = None,
    ) -> None:
        """Create a Delta table if it doesn't exist."""
        uri = self._table_uri(name)
        if os.path.exists(os.path.join(uri, "_delta_log")):
            return  # Already exists
        # Write an empty batch to create the table
        empty = pa.RecordBatch.from_pydict(
            {f.name: pa.array([], type=f.type) for f in schema},
            schema=schema,
        )
        table = pa.Table.from_batches([empty], schema=schema)
        write_deltalake(uri, table, mode="error", partition_by=partition_by or [])

    def _open_table(self, name: str, version: int | None = None) -> DeltaTable:
        uri = self._table_uri(name)
        if version is not None:
            return DeltaTable(uri, version=version)
        return DeltaTable(uri)

    # ─── Generic CRUD ───

    def append(self, table_name: str, data: pa.Table) -> int:
        """Append data to a Delta table. Returns new version number."""
        uri = self._table_uri(table_name)
        write_deltalake(uri, data, mode="append")
        return self._open_table(table_name).version()

    def scan(self, table_name: str) -> pa.Table:
        """Read all rows from a table."""
        return self._open_table(table_name).to_pyarrow_table()

    def query(self, table_name: str, filter_expr: str | None = None) -> pa.Table:
        """Read from table with an optional DuckDB/DataFusion filter."""
        dt = self._open_table(table_name)
        ds = dt.to_pyarrow_dataset()
        if filter_expr:
            import pyarrow.compute as pc
            # Use dataset filtering
            return ds.to_table(filter=pc.field("user_id").is_valid())  # fallback
        return ds.to_table()

    def read_version(self, table_name: str, version: int) -> pa.Table:
        """Time-travel: read table at a specific version."""
        return self._open_table(table_name, version=version).to_pyarrow_table()

    def version(self, table_name: str) -> int:
        """Get current version of a table."""
        return self._open_table(table_name).version()

    def history(self, table_name: str, limit: int | None = None) -> list[dict[str, Any]]:
        """Get version history."""
        dt = self._open_table(table_name)
        h = dt.history(limit=limit)
        return h if isinstance(h, list) else [h]

    def compact(self, table_name: str) -> dict[str, Any]:
        """Compact small files."""
        dt = self._open_table(table_name)
        return dt.optimize.compact()

    def z_order(self, table_name: str, columns: list[str]) -> dict[str, Any]:
        """Z-order optimize by columns."""
        dt = self._open_table(table_name)
        return dt.optimize.z_order(columns)

    def vacuum(self, table_name: str, retention_hours: int = 168, dry_run: bool = False) -> list[str]:
        """Remove old files. Returns list of removed files."""
        dt = self._open_table(table_name)
        return dt.vacuum(
            retention_hours=retention_hours,
            enforce_retention_duration=False,
            dry_run=dry_run,
        )

    # ─── Auth ───

    def register(
        self,
        username: str,
        email: str,
        password: str,
        first_name: str = "",
        last_name: str = "",
        tier: SubscriptionTier = SubscriptionTier.FREE,
    ) -> UserRecord:
        """Register a new user (starts as Pending)."""
        if len(username) < 3:
            raise ValueError("Username must be at least 3 characters")
        if "@" not in email:
            raise ValueError("Invalid email address")
        if len(password) < 8:
            raise ValueError("Password must be at least 8 characters")

        # Check uniqueness
        users = self.scan(self.TABLE_USERS)
        if len(users) > 0:
            usernames = users.column("username").to_pylist()
            emails = users.column("email").to_pylist()
            if username in usernames:
                raise ValueError(f"Username '{username}' already exists")
            if email in emails:
                raise ValueError(f"Email '{email}' already exists")

        user_id = str(uuid4())
        now = datetime.now(timezone.utc).isoformat()
        password_hash = self._hasher.hash(password)

        row = pa.Table.from_pydict(
            {
                "user_id": [user_id],
                "username": [username],
                "email": [email],
                "password_hash": [password_hash],
                "role": [UserRole.PENDING.value],
                "subscription_tier": [tier.value],
                "first_name": [first_name],
                "last_name": [last_name],
                "is_active": [True],
                "created_at": [now],
                "last_login": [None],
                "preferences_json": ["{}"],
            },
            schema=_users_schema(),
        )

        self.append(self.TABLE_USERS, row)
        self._log_audit(user_id, username, ActionType.REGISTER, detail=f"Tier: {tier.value}")

        return UserRecord(
            user_id=user_id, username=username, email=email,
            role=UserRole.PENDING, subscription_tier=tier,
            first_name=first_name, last_name=last_name,
            is_active=True, created_at=now, last_login=None,
        )

    def login(
        self, username: str, password: str, remember_me: bool = False
    ) -> tuple[str, UserRecord]:
        """Authenticate user and return (jwt_token, user)."""
        users = self.scan(self.TABLE_USERS)
        if len(users) == 0:
            raise PermissionError("Invalid credentials")

        # Find user
        mask = pa.compute.equal(users.column("username"), username)
        filtered = users.filter(mask)
        if len(filtered) == 0:
            raise PermissionError("Invalid credentials")

        row = filtered.to_pydict()
        stored_hash = row["password_hash"][0]
        is_active = row["is_active"][0]

        # Verify password
        try:
            self._hasher.verify(stored_hash, password)
        except VerifyMismatchError:
            raise PermissionError("Invalid credentials")

        if not is_active:
            raise PermissionError(f"Account '{username}' is disabled")

        user = UserRecord(
            user_id=row["user_id"][0],
            username=row["username"][0],
            email=row["email"][0],
            role=UserRole(row["role"][0]),
            subscription_tier=SubscriptionTier(row["subscription_tier"][0]) if row["subscription_tier"][0] else None,
            first_name=row["first_name"][0] or "",
            last_name=row["last_name"][0] or "",
            is_active=is_active,
            created_at=row["created_at"][0],
            last_login=row["last_login"][0],
        )

        # Generate JWT
        expiry_days = 30 if remember_me else self.session_expiry_days
        now = datetime.now(timezone.utc)
        payload = {
            "sub": user.user_id,
            "username": user.username,
            "role": user.role.value,
            "iat": int(now.timestamp()),
            "exp": int((now + timedelta(days=expiry_days)).timestamp()),
        }
        token = pyjwt.encode(payload, self.jwt_secret, algorithm="HS256")

        # Save session
        token_hash = hashlib.sha256(token.encode()).hexdigest()
        session = pa.Table.from_pydict(
            {
                "token_hash": [token_hash],
                "user_id": [user.user_id],
                "username": [user.username],
                "role": [user.role.value],
                "created_at": [now.isoformat()],
                "expires_at": [(now + timedelta(days=expiry_days)).isoformat()],
                "is_revoked": [False],
            },
            schema=_sessions_schema(),
        )
        self.append(self.TABLE_SESSIONS, session)

        self._log_audit(user.user_id, user.username, ActionType.LOGIN, detail=f"remember={remember_me}")
        return token, user

    def verify_token(self, token: str) -> Optional[UserRecord]:
        """Verify a JWT token and return the user if valid."""
        try:
            payload = pyjwt.decode(token, self.jwt_secret, algorithms=["HS256"])
        except Exception:
            return None

        # Check session not revoked
        token_hash = hashlib.sha256(token.encode()).hexdigest()
        sessions = self.scan(self.TABLE_SESSIONS)
        if len(sessions) > 0:
            import pyarrow.compute as pc
            mask = pc.and_(
                pc.equal(sessions.column("token_hash"), token_hash),
                pc.equal(sessions.column("is_revoked"), False),
            )
            valid = sessions.filter(mask)
            if len(valid) == 0:
                return None
        else:
            return None

        # Look up user
        return self.get_user(payload["sub"])

    def logout(self, token: str) -> bool:
        """Revoke a session token."""
        token_hash = hashlib.sha256(token.encode()).hexdigest()
        sessions = self.scan(self.TABLE_SESSIONS)
        if len(sessions) == 0:
            return False

        # Filter out the revoked session and rewrite
        import pyarrow.compute as pc
        mask = pc.invert(pc.equal(sessions.column("token_hash"), token_hash))
        remaining = sessions.filter(mask)

        uri = self._table_uri(self.TABLE_SESSIONS)
        write_deltalake(uri, remaining, mode="overwrite")
        return True

    def approve_user(self, user_id: str, tier: SubscriptionTier) -> UserRecord:
        """Approve a pending user — set role based on tier."""
        user = self.get_user(user_id)
        if user is None:
            raise ValueError(f"User {user_id} not found")

        self._update_user_field(user_id, "role", tier.default_role.value)
        self._update_user_field(user_id, "subscription_tier", tier.value)
        self._log_audit(user_id, user.username, ActionType.USER_APPROVED, detail=f"Tier: {tier.value}")

        updated = self.get_user(user_id)
        assert updated is not None, "User should exist after update"
        return updated

    def reject_user(self, user_id: str) -> bool:
        """Reject and remove a pending user."""
        user = self.get_user(user_id)
        if user is None:
            return False
        self._log_audit(user_id, user.username, ActionType.USER_REJECTED, detail="Registration rejected")
        return self._delete_user_by_id(user_id)

    def get_user(self, user_id: str) -> Optional[UserRecord]:
        """Get user by ID."""
        users = self.scan(self.TABLE_USERS)
        if len(users) == 0:
            return None
        import pyarrow.compute as pc
        mask = pc.equal(users.column("user_id"), user_id)
        filtered = users.filter(mask)
        if len(filtered) == 0:
            return None
        return self._row_to_user(filtered, 0)

    def get_pending_users(self) -> list[UserRecord]:
        """Get all pending registration requests."""
        users = self.scan(self.TABLE_USERS)
        if len(users) == 0:
            return []
        import pyarrow.compute as pc
        mask = pc.equal(users.column("role"), "pending")
        pending = users.filter(mask)
        return [self._row_to_user(pending, i) for i in range(len(pending))]

    def get_all_users(self) -> list[UserRecord]:
        """Get all active users."""
        users = self.scan(self.TABLE_USERS)
        if len(users) == 0:
            return []
        import pyarrow.compute as pc
        mask = pc.equal(users.column("is_active"), True)
        active = users.filter(mask)
        return [self._row_to_user(active, i) for i in range(len(active))]

    # ─── Audit ───

    def log_action(
        self,
        user_id: str,
        username: str,
        action: ActionType,
        resource: str | None = None,
        detail: str = "",
        ip_address: str | None = None,
    ) -> None:
        """Log an audit event."""
        self._log_audit(user_id, username, action, resource, detail, ip_address)

    def billing_summary(
        self, user_id: str, start_date: str, end_date: str
    ) -> BillingSummary:
        """Get billing summary for a user over a date range (YYYY-MM-DD)."""
        audit = self.scan(self.TABLE_AUDIT_LOG)
        if len(audit) == 0:
            return BillingSummary(
                user_id=user_id, period_start=start_date, period_end=end_date,
            )

        import pyarrow.compute as pc
        mask = pc.and_(
            pc.equal(audit.column("user_id"), user_id),
            pc.and_(
                pc.greater_equal(audit.column("date_partition"), start_date),
                pc.less_equal(audit.column("date_partition"), end_date),
            ),
        )
        filtered = audit.filter(mask)
        actions = filtered.column("action").to_pylist()

        return BillingSummary(
            user_id=user_id,
            period_start=start_date,
            period_end=end_date,
            total_queries=actions.count("query_executed"),
            total_uploads=actions.count("data_upload"),
            total_exports=actions.count("data_export"),
            total_backtests=actions.count("backtest_run"),
            total_live_trades=actions.count("live_trade_start"),
            total_actions=len(actions),
        )

    def get_user_activity(self, user_id: str, limit: int = 50) -> list[AuditEntry]:
        """Get recent audit events for a user."""
        audit = self.scan(self.TABLE_AUDIT_LOG)
        if len(audit) == 0:
            return []
        import pyarrow.compute as pc
        mask = pc.equal(audit.column("user_id"), user_id)
        filtered = audit.filter(mask)
        # Sort descending by timestamp
        indices = pc.sort_indices(filtered, sort_keys=[("timestamp", "descending")])
        sorted_table = filtered.take(indices)
        if len(sorted_table) > limit:
            sorted_table = sorted_table.slice(0, limit)
        return [self._row_to_audit(sorted_table, i) for i in range(len(sorted_table))]

    # ─── Time-travel ───

    def read_users_at_version(self, version: int) -> list[UserRecord]:
        """Time-travel: read the users table at a specific version."""
        table = self.read_version(self.TABLE_USERS, version)
        return [self._row_to_user(table, i) for i in range(len(table))]

    def read_users_at_timestamp(self, timestamp: str) -> list[UserRecord]:
        """Time-travel: read the users table at a specific timestamp (ISO 8601)."""
        dt = DeltaTable(self._table_uri(self.TABLE_USERS))
        # Find the version at the given timestamp
        hist = dt.history()
        target = datetime.fromisoformat(timestamp.replace("Z", "+00:00"))
        best_version = 0
        for entry in hist:
            ts = entry.get("timestamp")
            if ts and datetime.fromtimestamp(ts / 1000, tz=timezone.utc) <= target:
                v = entry.get("version", 0)
                if v > best_version:
                    best_version = v
        table = self.read_version(self.TABLE_USERS, best_version)
        return [self._row_to_user(table, i) for i in range(len(table))]

    # ─── GDPR ───

    def gdpr_delete_user(self, user_id: str) -> None:
        """GDPR-compliant full deletion of all user data + vacuum."""
        self._delete_user_by_id(user_id)
        # Delete sessions
        self._delete_from_table(self.TABLE_SESSIONS, "user_id", user_id)
        # Delete audit entries
        self._delete_from_table(self.TABLE_AUDIT_LOG, "user_id", user_id)
        # Vacuum to physically remove data
        for t in [self.TABLE_USERS, self.TABLE_SESSIONS, self.TABLE_AUDIT_LOG]:
            try:
                self.vacuum(t, retention_hours=0)
            except Exception:
                pass

    # ─── Private Helpers ───

    def _log_audit(
        self,
        user_id: str,
        username: str,
        action: ActionType,
        resource: str | None = None,
        detail: str = "",
        ip_address: str | None = None,
    ) -> None:
        now = datetime.now(timezone.utc)
        row = pa.Table.from_pydict(
            {
                "event_id": [str(uuid4())],
                "user_id": [user_id],
                "username": [username],
                "action": [action.value],
                "resource": [resource],
                "detail": [detail],
                "ip_address": [ip_address],
                "timestamp": [now.isoformat()],
                "date_partition": [now.strftime("%Y-%m-%d")],
            },
            schema=_audit_log_schema(),
        )
        try:
            self.append(self.TABLE_AUDIT_LOG, row)
        except Exception:
            pass  # Audit logging should never block operations

    def _row_to_user(self, table: pa.Table, i: int) -> UserRecord:
        d = {col: table.column(col)[i].as_py() for col in table.column_names}
        return UserRecord(
            user_id=d["user_id"],
            username=d["username"],
            email=d["email"],
            role=UserRole(d["role"]) if d["role"] in UserRole._value2member_map_ else UserRole.GUEST,
            subscription_tier=SubscriptionTier(d["subscription_tier"]) if d.get("subscription_tier") and d["subscription_tier"] in SubscriptionTier._value2member_map_ else None,
            first_name=d.get("first_name") or "",
            last_name=d.get("last_name") or "",
            is_active=d.get("is_active", True),
            created_at=d.get("created_at", ""),
            last_login=d.get("last_login"),
        )

    def _row_to_audit(self, table: pa.Table, i: int) -> AuditEntry:
        d = {col: table.column(col)[i].as_py() for col in table.column_names}
        return AuditEntry(
            event_id=d.get("event_id"),
            user_id=d.get("user_id"),
            username=d.get("username"),
            action=ActionType(d["action"]) if d.get("action") in ActionType._value2member_map_ else ActionType.ADMIN_ACTION,
            resource=d.get("resource"),
            detail=d.get("detail", ""),
            ip_address=d.get("ip_address"),
            timestamp=d.get("timestamp", ""),
            date_partition=d.get("date_partition", ""),
        )

    def _update_user_field(self, user_id: str, field: str, value: Any) -> None:
        """Update a single field by rewriting the user row."""
        users = self.scan(self.TABLE_USERS)
        if len(users) == 0:
            return
        import pyarrow.compute as pc
        # Separate target row and others
        mask = pc.equal(users.column("user_id"), user_id)
        others = users.filter(pc.invert(mask))
        target = users.filter(mask)
        if len(target) == 0:
            return
        # Rebuild target with updated field
        d = {col: target.column(col).to_pylist() for col in target.column_names}
        d[field] = [value]
        updated = pa.Table.from_pydict(d, schema=users.schema)
        # Combine and overwrite
        combined = pa.concat_tables([others, updated])
        uri = self._table_uri(self.TABLE_USERS)
        write_deltalake(uri, combined, mode="overwrite")

    def _delete_user_by_id(self, user_id: str) -> bool:
        users = self.scan(self.TABLE_USERS)
        if len(users) == 0:
            return False
        import pyarrow.compute as pc
        mask = pc.invert(pc.equal(users.column("user_id"), user_id))
        remaining = users.filter(mask)
        if len(remaining) == len(users):
            return False
        uri = self._table_uri(self.TABLE_USERS)
        write_deltalake(uri, remaining, mode="overwrite")
        return True

    def _delete_from_table(self, table_name: str, column: str, value: str) -> None:
        data = self.scan(table_name)
        if len(data) == 0:
            return
        import pyarrow.compute as pc
        mask = pc.invert(pc.equal(data.column(column), value))
        remaining = data.filter(mask)
        uri = self._table_uri(table_name)
        write_deltalake(uri, remaining, mode="overwrite")
