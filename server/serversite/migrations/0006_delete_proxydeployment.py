from django.db import migrations


class Migration(migrations.Migration):

    dependencies = [("serversite", "0005_proxydeployment")]

    operations = [migrations.DeleteModel(name="ProxyDeployment")]
