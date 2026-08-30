(ns paredit.corpus
  "A layout corpus fixture: namespace form, maps, vectors, sets, metadata,
  destructuring, threading macros, and the reader shorthands."
  (:require [clojure.set :as set] [clojure.string :as str]))

(def ^:private default-options
  {:retries 3 :timeout-ms 500 :tags #{:corpus :layout}})

(defrecord Account [id balance])

(defn classify
  "Name the magnitude of n."
  [n]
  (cond
    (neg? n) :negative
    (zero? n) :zero
    (> n 100) :large
    :else :positive))

(defn transfer
  "Move amount from one account to another, returning both."
  [{:keys [balance] :as from} to amount]
  (when (< balance amount)
    (throw (ex-info "insufficient funds" {:account (:id from) :amount amount})))
  [(update from :balance - amount) (update to :balance + amount)])

(defn summarise [accounts]
  (let [total (reduce + (map :balance accounts))
        labels (->> accounts
                    (remove (comp zero? :balance))
                    (map #(str (:id %) ":" (:balance %))))]
    (str/join ", " (conj (vec labels) (str "total " total)))))

(defmulti render :kind)

(defmethod render :text [{:keys [body]}] body)

(defmethod render :default [node] (str "<unknown " (name (:kind node)) ">"))

(defn report [items]
  (doseq [item items
          :when (:active? item)]
    (println (:name item) "->" (render item))))

(defn- retry [f attempts]
  (loop [attempt 1]
    (let [result (try
                   (f)
                   (catch Exception _ ::failed))]
      (if (or (not= result ::failed) (>= attempt attempts))
        result
        (recur (inc attempt))))))

(defn tags-in-common [a b] (set/intersection (:tags a #{}) (:tags b #{})))
