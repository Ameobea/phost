from django.test import TestCase
from django.db import transaction

from .models import StaticDeployment
from .views import get_or_none


class DummyException(Exception):
    pass


TEST_SUBDOMAIN = "__test_subdomain"


class TestAtomicTransactionGenericErrorHandling(TestCase):
    """ Verify that random exceptions raised in `transaction.atomic()` blocks still cause the
    transaction to be rolled back. """

    def test_fail(self):
        with self.assertRaises(DummyException):
            with transaction.atomic():
                deployment = StaticDeployment(name="Test Deployment", subdomain=TEST_SUBDOMAIN)
                deployment.save()
                raise DummyException("generic exception")

        assert get_or_none(StaticDeployment, subdomain=TEST_SUBDOMAIN, do_raise=False) is None


class EmptyQuery(TestCase):
    def test_empty_falsey(self):
        assert not StaticDeployment.objects.filter(name="__non-existant-name")
