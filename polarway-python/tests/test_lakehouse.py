"""Tests for the Polarway Lakehouse Python client.

Requires: pip install deltalake argon2-cffi PyJWT pyarrow pytest
"""

import os
import tempfile

import pytest

from polarway.lakehouse import (
    ActionType,
    LakehouseClient,
    SubscriptionTier,
    UserRole,
)


@pytest.fixture
def client():
    """Create a LakehouseClient with a temporary directory."""
    with tempfile.TemporaryDirectory() as tmpdir:
        c = LakehouseClient(tmpdir, jwt_secret="test-jwt-secret-key-for-tests")
        yield c


class TestRegistration:
    def test_register_user(self, client: LakehouseClient):
        user = client.register(
            "albert", "albert@example.com", "StrongP@ss1",
            first_name="Albert", last_name="Smith",
            tier=SubscriptionTier.PIONEER,
        )
        assert user.username == "albert"
        assert user.role == UserRole.PENDING
        assert user.subscription_tier == SubscriptionTier.PIONEER

    def test_duplicate_username(self, client: LakehouseClient):
        client.register("bernoulli", "bernoulli@example.com", "P@ssword123")
        with pytest.raises(ValueError, match="already exists"):
            client.register("bernoulli", "bernoulli2@example.com", "P@ssword123")

    def test_duplicate_email(self, client: LakehouseClient):
        client.register("carl", "carl@example.com", "P@ssword123")
        with pytest.raises(ValueError, match="already exists"):
            client.register("carl2", "carl@example.com", "P@ssword123")

    def test_short_username(self, client: LakehouseClient):
        with pytest.raises(ValueError, match="3 characters"):
            client.register("ab", "ab@example.com", "P@ssword123")

    def test_weak_password(self, client: LakehouseClient):
        with pytest.raises(ValueError, match="8 characters"):
            client.register("diana", "diana@example.com", "short")


class TestLogin:
    def test_login_success(self, client: LakehouseClient):
        client.register("eve", "eve@example.com", "SecureP@ss99")
        token, user = client.login("eve", "SecureP@ss99")
        assert token
        assert user.username == "eve"

    def test_login_wrong_password(self, client: LakehouseClient):
        client.register("frank", "frank@example.com", "MyP@ssword1")
        with pytest.raises(PermissionError):
            client.login("frank", "WrongPassword")

    def test_login_nonexistent_user(self, client: LakehouseClient):
        with pytest.raises(PermissionError):
            client.login("nobody", "P@ssword123")


class TestTokenVerification:
    def test_verify_valid_token(self, client: LakehouseClient):
        client.register("grace", "grace@example.com", "V@lidPass1")
        token, _ = client.login("grace", "V@lidPass1")
        user = client.verify_token(token)
        assert user is not None
        assert user.username == "grace"

    def test_verify_invalid_token(self, client: LakehouseClient):
        user = client.verify_token("invalid.jwt.token")
        assert user is None

    def test_logout_invalidates_token(self, client: LakehouseClient):
        client.register("henry", "henry@example.com", "LogoutP@ss1")
        token, _ = client.login("henry", "LogoutP@ss1")
        assert client.logout(token)
        user = client.verify_token(token)
        assert user is None


class TestApproval:
    def test_approve_user(self, client: LakehouseClient):
        u = client.register("iris", "iris@example.com", "Appr0veMe!")
        approved = client.approve_user(u.user_id, SubscriptionTier.PROFESSIONAL)
        assert approved.role == UserRole.TRADER
        assert approved.subscription_tier == SubscriptionTier.PROFESSIONAL

    def test_reject_user(self, client: LakehouseClient):
        u = client.register("jack", "jack@example.com", "R3jectMe!")
        assert client.reject_user(u.user_id)
        assert client.get_user(u.user_id) is None

    def test_pending_users_list(self, client: LakehouseClient):
        client.register("kate", "kate@example.com", "P3ndingMe!")
        client.register("luke", "luke@example.com", "P3ndingMe!")
        pending = client.get_pending_users()
        assert len(pending) == 2


class TestTimeTravel:
    def test_read_at_version(self, client: LakehouseClient):
        v0 = client.version(client.TABLE_USERS)
        client.register("tt1", "tt1@example.com", "TimeTrav3l!")
        client.register("tt2", "tt2@example.com", "TimeTrav3l!")

        # Current version has 2 users
        current = client.scan(client.TABLE_USERS)
        assert len(current) == 2

        # Version after first register should have 1 user
        v1_table = client.read_version(client.TABLE_USERS, v0 + 1)
        assert len(v1_table) == 1

    def test_history(self, client: LakehouseClient):
        client.register("hist1", "hist1@example.com", "Hist0ryMe!")
        h = client.history(client.TABLE_USERS)
        assert len(h) >= 2  # create + append


class TestOptimization:
    def test_compact(self, client: LakehouseClient):
        for i in range(5):
            client.register(f"compact{i}", f"compact{i}@example.com", f"C0mpact{i}!")
        result = client.compact(client.TABLE_USERS)
        assert isinstance(result, dict)

    def test_vacuum_dry_run(self, client: LakehouseClient):
        client.register("vac1", "vac1@example.com", "V@cuum123!")
        result = client.vacuum(client.TABLE_USERS, retention_hours=0, dry_run=True)
        assert isinstance(result, list)


class TestAudit:
    def test_log_action(self, client: LakehouseClient):
        client.register("audit1", "audit1@example.com", "Aud1tMe!!")
        client.log_action(
            "user-123", "audit1", ActionType.BACKTEST_RUN,
            resource="strategy-456", detail="BTC/USD backtest",
        )
        activity = client.get_user_activity("user-123", limit=10)
        assert len(activity) >= 1

    def test_billing_summary(self, client: LakehouseClient):
        uid = "billing-user"
        for action in [ActionType.QUERY_EXECUTED, ActionType.BACKTEST_RUN, ActionType.LOGIN]:
            client.log_action(uid, "biller", action, detail="test")

        summary = client.billing_summary(uid, "2020-01-01", "2030-12-31")
        assert summary.total_queries == 1
        assert summary.total_backtests == 1
        assert summary.total_actions == 3  # query + backtest + login


class TestGDPR:
    def test_gdpr_delete(self, client: LakehouseClient):
        u = client.register("gdpr1", "gdpr1@example.com", "Gdpr!Delete1")
        client.login("gdpr1", "Gdpr!Delete1")
        client.gdpr_delete_user(u.user_id)
        assert client.get_user(u.user_id) is None


class TestEnums:
    def test_role_hierarchy(self):
        assert UserRole.ADMIN.has_permission(UserRole.TRADER)
        assert not UserRole.GUEST.has_permission(UserRole.TRADER)

    def test_tier_default_role(self):
        assert SubscriptionTier.FREE.default_role == UserRole.REGISTERED
        assert SubscriptionTier.PIONEER.default_role == UserRole.TRADER

    def test_action_billable(self):
        assert ActionType.BACKTEST_RUN.is_billable
        assert not ActionType.LOGIN.is_billable
