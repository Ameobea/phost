# Generated by Django 2.1.1 on 2018-09-19 16:43

from django.db import migrations, models


class Migration(migrations.Migration):

    dependencies = [
        ('serversite', '0002_auto_20180911_0341'),
    ]

    operations = [
        migrations.CreateModel(
            name='DeploymentCategory',
            fields=[
                ('id', models.AutoField(auto_created=True, primary_key=True, serialize=False, verbose_name='ID')),
                ('category', models.CharField(max_length=64, unique=True)),
            ],
        ),
        migrations.AddField(
            model_name='staticdeployment',
            name='categories',
            field=models.ManyToManyField(to='serversite.DeploymentCategory'),
        ),
    ]
