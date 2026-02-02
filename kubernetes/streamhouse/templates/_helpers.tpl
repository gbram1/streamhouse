{{/*
Expand the name of the chart.
*/}}
{{- define "streamhouse.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
*/}}
{{- define "streamhouse.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "streamhouse.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "streamhouse.labels" -}}
helm.sh/chart: {{ include "streamhouse.chart" . }}
{{ include "streamhouse.selectorLabels" . }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- with .Values.commonLabels }}
{{ toYaml . }}
{{- end }}
{{- end }}

{{/*
Selector labels
*/}}
{{- define "streamhouse.selectorLabels" -}}
app.kubernetes.io/name: {{ include "streamhouse.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Agent labels
*/}}
{{- define "streamhouse.agent.labels" -}}
{{ include "streamhouse.labels" . }}
app.kubernetes.io/component: agent
{{- end }}

{{/*
Agent selector labels
*/}}
{{- define "streamhouse.agent.selectorLabels" -}}
{{ include "streamhouse.selectorLabels" . }}
app.kubernetes.io/component: agent
{{- end }}

{{/*
Schema Registry labels
*/}}
{{- define "streamhouse.schemaRegistry.labels" -}}
{{ include "streamhouse.labels" . }}
app.kubernetes.io/component: schema-registry
{{- end }}

{{/*
Schema Registry selector labels
*/}}
{{- define "streamhouse.schemaRegistry.selectorLabels" -}}
{{ include "streamhouse.selectorLabels" . }}
app.kubernetes.io/component: schema-registry
{{- end }}

{{/*
UI labels
*/}}
{{- define "streamhouse.ui.labels" -}}
{{ include "streamhouse.labels" . }}
app.kubernetes.io/component: ui
{{- end }}

{{/*
UI selector labels
*/}}
{{- define "streamhouse.ui.selectorLabels" -}}
{{ include "streamhouse.selectorLabels" . }}
app.kubernetes.io/component: ui
{{- end }}

{{/*
Create the name of the service account to use
*/}}
{{- define "streamhouse.serviceAccountName" -}}
{{- if .Values.serviceAccount.create }}
{{- default (include "streamhouse.fullname" .) .Values.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.serviceAccount.name }}
{{- end }}
{{- end }}

{{/*
Return the proper image name
*/}}
{{- define "streamhouse.image" -}}
{{- $registryName := .Values.global.imageRegistry | default "" -}}
{{- $repositoryName := .repository -}}
{{- $tag := .tag | default $.Chart.AppVersion -}}
{{- if $registryName }}
{{- printf "%s/%s:%s" $registryName $repositoryName $tag -}}
{{- else }}
{{- printf "%s:%s" $repositoryName $tag -}}
{{- end }}
{{- end }}

{{/*
PostgreSQL connection string
*/}}
{{- define "streamhouse.postgresql.url" -}}
{{- if .Values.postgresql.enabled }}
{{- printf "postgres://%s:%s@%s-postgresql:5432/%s" .Values.postgresql.auth.username .Values.postgresql.auth.password (include "streamhouse.fullname" .) .Values.postgresql.auth.database }}
{{- else }}
{{- printf "postgres://%s:%s@%s:%d/%s" .Values.postgresql.external.username .Values.postgresql.external.password .Values.postgresql.external.host (.Values.postgresql.external.port | int) .Values.postgresql.external.database }}
{{- end }}
{{- end }}

{{/*
S3 endpoint
*/}}
{{- define "streamhouse.s3.endpoint" -}}
{{- if .Values.minio.enabled }}
{{- printf "http://%s-minio:9000" (include "streamhouse.fullname" .) }}
{{- else if .Values.minio.external.endpoint }}
{{- .Values.minio.external.endpoint }}
{{- else }}
{{- "" }}
{{- end }}
{{- end }}

{{/*
S3 bucket name
*/}}
{{- define "streamhouse.s3.bucket" -}}
{{- if .Values.minio.enabled }}
{{- .Values.agent.storage.bucket }}
{{- else if .Values.minio.external.bucket }}
{{- .Values.minio.external.bucket }}
{{- else }}
{{- .Values.agent.storage.bucket }}
{{- end }}
{{- end }}
